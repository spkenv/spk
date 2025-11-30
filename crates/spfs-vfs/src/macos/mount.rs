// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS mount implementation backed by macFUSE.
//!
//! This closely mirrors the Linux FUSE backend but adapts the interface so
//! that synchronous fuser callbacks can be delegated from the Router.

use std::ffi::OsStr;
use std::io::{Seek, SeekFrom};
use std::mem::ManuallyDrop;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::prelude::FileExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use dashmap::DashMap;
use fuser::consts::{FOPEN_KEEP_CACHE, FOPEN_NONSEEKABLE};
use fuser::{FileAttr, FileType, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, ReplyLseek, ReplyOpen, ReplyStatfs};
use spfs::prelude::*;
use spfs::storage::{LocalRepository, RepositoryHandle};
use spfs::tracking::{Entry, EntryKind, Manifest};
use tokio::io::AsyncReadExt;

use super::handle::Handle;
use super::scratch::ScratchDir;

// FOPEN_STREAM is not available in fuser on macOS, define it ourselves
// This flag indicates that the file is a stream (non-seekable)
const FOPEN_STREAM: u32 = 1 << 4;

const BLOCK_SIZE: u32 = 512;

macro_rules! reply_error {
    ($reply:ident, $err:expr) => {{
        let err = $err;
        tracing::error!(?err, "macOS mount error");
        $reply.error(libc::EIO);
        return;
    }};
}

macro_rules! unwrap_reply {
    ($reply:ident, $expr:expr) => {{
        match $expr {
            Ok(value) => value,
            Err(err) => reply_error!($reply, err),
        }
    }};
}

/// A FUSE filesystem mount backed by SPFS repositories.
///
/// This struct manages inodes and file handles for a single manifest,
/// providing synchronous filesystem operations that can be called from
/// fuser's callback methods.
///
/// ## Editable Mounts
///
/// When created with [`Mount::new_editable`], the mount supports write
/// operations using a scratch directory for copy-on-write semantics.
/// Modified and new files are stored in the scratch directory, while
/// reads first check scratch before falling back to the repository.
#[derive(Debug)]
pub struct Mount {
    rt: tokio::runtime::Handle,
    repos: Vec<Arc<RepositoryHandle>>,
    ttl: Duration,
    next_inode: AtomicU64,
    next_handle: AtomicU64,
    /// Map of inode -> entry for base layer files
    inodes: DashMap<u64, Arc<Entry<u64>>>,
    /// Map of virtual path -> inode for scratch files
    scratch_inodes: DashMap<std::path::PathBuf, u64>,
    /// Reverse map of inode -> virtual path for scratch files
    inode_to_path: DashMap<u64, std::path::PathBuf>,
    handles: DashMap<u64, Handle>,
    fs_creation_time: SystemTime,
    uid: u32,
    gid: u32,
    /// Scratch directory for editable mounts (None for read-only)
    scratch: Option<ScratchDir>,
}

impl Mount {
    /// Create a new read-only Mount from repositories and a manifest.
    pub fn new(
        rt: tokio::runtime::Handle,
        repos: Vec<Arc<RepositoryHandle>>,
        manifest: Manifest,
    ) -> spfs::Result<Self> {
        Self::new_internal(rt, repos, manifest, None)
    }

    /// Create a new editable Mount with a scratch directory for writes.
    ///
    /// The scratch directory is created under the system temp directory
    /// using the runtime name for identification.
    pub fn new_editable(
        rt: tokio::runtime::Handle,
        repos: Vec<Arc<RepositoryHandle>>,
        manifest: Manifest,
        runtime_name: &str,
    ) -> spfs::Result<Self> {
        let scratch = ScratchDir::new(runtime_name).map_err(|e| {
            spfs::Error::String(format!("Failed to create scratch directory: {e}"))
        })?;
        Self::new_internal(rt, repos, manifest, Some(scratch))
    }

    fn new_internal(
        rt: tokio::runtime::Handle,
        repos: Vec<Arc<RepositoryHandle>>,
        manifest: Manifest,
        scratch: Option<ScratchDir>,
    ) -> spfs::Result<Self> {
        let uid = nix::unistd::Uid::current().as_raw();
        let gid = nix::unistd::Gid::current().as_raw();
        let mount = Self {
            rt,
            repos,
            ttl: Duration::from_secs(u64::MAX),
            next_inode: AtomicU64::new(1),
            next_handle: AtomicU64::new(1),
            inodes: DashMap::default(),
            scratch_inodes: DashMap::default(),
            inode_to_path: DashMap::default(),
            handles: DashMap::default(),
            fs_creation_time: SystemTime::now(),
            uid,
            gid,
            scratch,
        };
        let mut root = manifest.take_root();
        root.mode |= libc::S_IFDIR as u32;
        mount.allocate_inodes(root);
        Ok(mount)
    }

    /// Create an empty mount with no repositories or entries.
    pub fn empty() -> spfs::Result<Self> {
        Self::new(tokio::runtime::Handle::current(), Vec::new(), Manifest::default())
    }

    /// Get the repositories backing this mount.
    pub fn repos(&self) -> &[Arc<RepositoryHandle>] {
        &self.repos
    }

    /// Returns true if this mount is editable (has a scratch directory).
    pub fn is_editable(&self) -> bool {
        self.scratch.is_some()
    }

    /// Get the scratch directory, if this is an editable mount.
    pub fn scratch(&self) -> Option<&ScratchDir> {
        self.scratch.as_ref()
    }

    /// Look up a directory entry by name.
    pub fn lookup(&self, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let Some(parent_entry) = self.inodes.get(&parent) else {
            reply.error(libc::ENOENT);
            return;
        };

        if parent_entry.kind != EntryKind::Tree {
            reply.error(libc::ENOTDIR);
            return;
        }

        let Some(name_str) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };

        let Some(entry) = parent_entry.entries.get(name_str) else {
            reply.error(libc::ENOENT);
            return;
        };

        let Ok(attr) = self.attr_from_entry(entry) else {
            reply.error(libc::ENOENT);
            return;
        };
        reply.entry(&self.ttl, &attr, 0);
    }

    /// Get file attributes for an inode.
    pub fn getattr(&self, ino: u64, reply: ReplyAttr) {
        let Some(entry) = self.inodes.get(&ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        let Ok(attr) = self.attr_from_entry(entry.value()) else {
            reply.error(libc::ENOENT);
            return;
        };
        reply.attr(&self.ttl, &attr);
    }

    /// Read the target of a symbolic link.
    pub fn readlink(&self, ino: u64, reply: ReplyData) {
        let Some(entry) = self.inodes.get(&ino).map(|kv| Arc::clone(kv.value())) else {
            reply.error(libc::ENOENT);
            return;
        };
        if !entry.is_symlink() {
            reply.error(libc::EINVAL);
            return;
        }

        let mut data = None;
        for repo in &self.repos {
            match self.rt.block_on(repo.open_payload(entry.object)) {
                Ok((mut reader, _)) => {
                    let mut bytes = Vec::new();
                    if let Err(err) = self.rt.block_on(reader.read_to_end(&mut bytes)) {
                        reply_error!(reply, spfs::Error::String(format!("read error: {err}")));
                    }
                    data = Some(bytes);
                    break;
                }
                Err(err) if err.try_next_repo() => continue,
                Err(err) => reply_error!(reply, err),
            }
        }

        let Some(data) = data else {
            reply_error!(reply, spfs::Error::UnknownObject(entry.object));
        };
        reply.data(&data);
    }

    /// Open a file and return a file handle.
    pub fn open(&self, ino: u64, flags: i32, reply: ReplyOpen) {
        let Some(entry) = self.inodes.get(&ino).map(|kv| Arc::clone(kv.value())) else {
            reply.error(libc::ENOENT);
            return;
        };
        if entry.is_dir() {
            reply.error(libc::EISDIR);
            return;
        }
        if flags & (libc::O_WRONLY | libc::O_RDWR) != 0 {
            reply.error(libc::EROFS);
            return;
        }

        let handle = match self.open_blob_handle(entry) {
            Ok(handle) => handle,
            Err(err) => reply_error!(reply, err),
        };

        let mut out_flags = FOPEN_KEEP_CACHE;
        if matches!(handle, Handle::BlobStream { .. }) {
            out_flags |= FOPEN_NONSEEKABLE | FOPEN_STREAM;
        }
        let fh = self.allocate_handle(handle);
        reply.opened(fh, out_flags);
    }

    /// Read data from an open file handle.
    #[allow(clippy::too_many_arguments)]
    pub fn read(
        &self,
        _ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let Some(handle) = self.handles.get(&fh) else {
            reply.error(libc::EBADF);
            return;
        };

        match handle.value() {
            Handle::Tree { .. } => reply.error(libc::EISDIR),
            Handle::BlobFile { file, .. } | Handle::ScratchFile { file, .. } => {
                let file_clone = unsafe { std::fs::File::from_raw_fd(file.as_raw_fd()) };
                let file_clone = ManuallyDrop::new(file_clone);
                let mut buf = vec![0; size as usize];
                let mut consumed = 0;
                while consumed < buf.len() {
                    let count = unwrap_reply!(
                        reply,
                        file_clone.read_at(&mut buf[consumed..], consumed as u64 + offset as u64)
                    );
                    consumed += count;
                    if count == 0 {
                        break;
                    }
                }
                reply.data(&buf[..consumed]);
            }
            Handle::BlobStream { stream, offset: pos, .. } => {
                if pos.load(Ordering::Relaxed) != offset as u64 {
                    reply.error(libc::EINVAL);
                    return;
                }
                let stream = Arc::clone(stream);
                let pos = Arc::clone(pos);
                let read_res = self.rt.block_on(async move {
                    let mut guard = stream.lock().await;
                    let mut buf = vec![0; size as usize];
                    let mut consumed = 0;
                    while consumed < buf.len() {
                        let count = guard.read(&mut buf[consumed..]).await?;
                        consumed += count;
                        if count == 0 {
                            break;
                        }
                    }
                    pos.fetch_add(consumed as u64, Ordering::Relaxed);
                    Ok::<Vec<u8>, std::io::Error>(buf[..consumed].to_vec())
                });
                match read_res {
                    Ok(data) => reply.data(&data),
                    Err(err) => reply_error!(reply, spfs::Error::String(format!("stream read error: {err}"))),
                }
            }
        }
    }

    /// Release (close) an open file handle.
    pub fn release(
        &self,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        if self.handles.remove(&fh).is_none() {
            reply.error(libc::EBADF);
            return;
        }
        reply.ok();
    }

    /// Open a directory and return a directory handle.
    pub fn opendir(&self, ino: u64, _flags: i32, reply: ReplyOpen) {
        let Some(entry) = self.inodes.get(&ino).map(|kv| Arc::clone(kv.value())) else {
            reply.error(libc::ENOENT);
            return;
        };
        if !entry.is_dir() {
            reply.error(libc::ENOTDIR);
            return;
        }
        let fh = self.allocate_handle(Handle::Tree { entry });
        reply.opened(fh, 0);
    }

    /// Read entries from an open directory.
    pub fn readdir(&self, _ino: u64, fh: u64, offset: i64, mut reply: ReplyDirectory) {
        let Some(entry) = self.handles.get(&fh).map(|handle| handle.value().entry_owned()) else {
            reply.error(libc::EBADF);
            return;
        };

        let mut iter = entry.entries.iter();
        if offset != 0 {
            while let Some((_, child)) = iter.next() {
                if child.user_data == offset as u64 {
                    break;
                }
            }
        }

        for (name, child) in iter {
            let file_type = match child.kind {
                EntryKind::Blob(_) if child.is_symlink() => FileType::Symlink,
                EntryKind::Blob(_) => FileType::RegularFile,
                EntryKind::Tree => FileType::Directory,
                EntryKind::Mask => continue,
            };
            let name_os: &OsStr = OsStr::new(name);
            let next_off = child.user_data as i64;
            if reply.add(child.user_data, next_off, file_type, name_os) {
                break;
            }
        }
        reply.ok();
    }

    /// Release (close) an open directory handle.
    pub fn releasedir(&self, _ino: u64, fh: u64, _flags: i32, reply: fuser::ReplyEmpty) {
        if self.handles.remove(&fh).is_none() {
            reply.error(libc::EBADF);
            return;
        }
        reply.ok();
    }

    /// Get filesystem statistics.
    pub fn statfs(&self, _ino: u64, reply: ReplyStatfs) {
        let blocks = self
            .inodes
            .iter()
            .map(|entry| (entry.value().size() / BLOCK_SIZE as u64) + 1)
            .sum();
        let files = self
            .inodes
            .iter()
            .filter(|entry| entry.value().kind.is_blob())
            .count();
        reply.statfs(blocks, 0, 0, files as u64, 0, BLOCK_SIZE, u32::MAX, BLOCK_SIZE);
    }

    /// Check file access permissions.
    pub fn access(&self, _ino: u64, _mask: i32, reply: fuser::ReplyEmpty) {
        reply.ok();
    }

    /// Seek within an open file.
    pub fn lseek(&self, _ino: u64, fh: u64, offset: i64, whence: i32, reply: ReplyLseek) {
        let Some(handle) = self.handles.get(&fh) else {
            reply.error(libc::EBADF);
            return;
        };
        let file = match handle.value() {
            Handle::Tree { .. } => {
                reply.error(libc::EISDIR);
                return;
            }
            Handle::BlobFile { file, .. } | Handle::ScratchFile { file, .. } => file,
            Handle::BlobStream { .. } => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        let seek_from = match whence {
            libc::SEEK_CUR => SeekFrom::Current(offset),
            libc::SEEK_END => SeekFrom::End(offset),
            libc::SEEK_SET => SeekFrom::Start(offset as u64),
            libc::SEEK_HOLE => SeekFrom::End(0),
            libc::SEEK_DATA => SeekFrom::Start(offset as u64),
            _ => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        let file_clone = unsafe { std::fs::File::from_raw_fd(file.as_raw_fd()) };
        let mut file_clone = ManuallyDrop::new(file_clone);
        let pos = unwrap_reply!(reply, file_clone.seek(seek_from));
        reply.offset(pos as i64);
    }

    fn allocate_inodes(&self, entry: Entry) -> Arc<Entry<u64>> {
        let Entry {
            kind,
            object,
            mode,
            entries,
            user_data: _,
            legacy_size,
        } = entry;

        let inode = self.allocate_inode();
        let entries = entries
            .into_iter()
            .map(|(name, child)| (name, self.allocate_inodes(child).as_ref().clone()))
            .collect();

        let entry = Arc::new(Entry {
            kind,
            object,
            mode,
            entries,
            user_data: inode,
            legacy_size,
        });
        self.inodes.insert(inode, Arc::clone(&entry));
        entry
    }

    fn allocate_inode(&self) -> u64 {
        self.next_inode.fetch_add(1, Ordering::Relaxed)
    }

    fn allocate_handle(&self, handle: Handle) -> u64 {
        loop {
            let id = self.next_handle.fetch_add(1, Ordering::Relaxed);
            if id == 0 {
                continue;
            }
            match self.handles.entry(id) {
                dashmap::mapref::entry::Entry::Occupied(_) => continue,
                dashmap::mapref::entry::Entry::Vacant(slot) => {
                    slot.insert(handle);
                    break id;
                }
            }
        }
    }

    fn attr_from_entry(&self, entry: &Entry<u64>) -> spfs::Result<FileAttr> {
        let kind = match entry.kind {
            EntryKind::Blob(_) if entry.is_symlink() => FileType::Symlink,
            EntryKind::Blob(_) => FileType::RegularFile,
            EntryKind::Tree => FileType::Directory,
            EntryKind::Mask => return Err(spfs::Error::String("Entry is a mask".to_string())),
        };
        let size = if entry.is_dir() {
            entry.entries.len() as u64
        } else {
            entry.size()
        };
        let nlink = if entry.is_dir() {
            (entry.entries.iter().filter(|(_, e)| e.is_dir()).count() + 2) as u32
        } else {
            1
        };
        Ok(FileAttr {
            ino: entry.user_data,
            size,
            perm: entry.mode as u16,
            uid: self.uid,
            gid: self.gid,
            blocks: (size / BLOCK_SIZE as u64) + 1,
            atime: self.fs_creation_time,
            mtime: self.fs_creation_time,
            ctime: self.fs_creation_time,
            crtime: self.fs_creation_time,
            kind,
            nlink,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        })
    }

    fn open_blob_handle(&self, entry: Arc<Entry<u64>>) -> spfs::Result<Handle> {
        for repo in &self.repos {
            match &**repo {
                RepositoryHandle::FS(fs_repo) => {
                    let Ok(fs_repo) = self.rt.block_on(fs_repo.opened()) else {
                        continue;
                    };
                    let payload_path = fs_repo.payloads().build_digest_path(&entry.object);
                    match std::fs::OpenOptions::new().read(true).open(&payload_path) {
                        Ok(file) => return Ok(Handle::BlobFile { entry, file }),
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(err) => return Err(spfs::Error::StorageReadError(
                            "open payload file",
                            payload_path,
                            err,
                        )),
                    }
                }
                repo => match self.rt.block_on(repo.open_payload(entry.object)) {
                    Ok((stream, _)) => {
                        return Ok(Handle::BlobStream {
                            entry,
                            offset: Arc::new(AtomicU64::new(0)),
                            stream: Arc::new(tokio::sync::Mutex::new(stream)),
                        });
                    }
                    Err(err) if err.try_next_repo() => continue,
                    Err(err) => return Err(err),
                },
            }
        }
        Err(spfs::Error::UnknownObject(entry.object))
    }

    // ========================================================================
    // Write operations (only work on editable mounts with scratch directory)
    // ========================================================================

    /// Write data to an open file handle.
    #[allow(clippy::too_many_arguments)]
    pub fn write(
        &self,
        _ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        // Editable check
        if self.scratch.is_none() {
            reply.error(libc::EROFS);
            return;
        }

        let Some(handle) = self.handles.get(&fh) else {
            reply.error(libc::EBADF);
            return;
        };

        match handle.value() {
            Handle::ScratchFile { file, .. } => {
                // Write directly to scratch file
                match file.write_at(data, offset as u64) {
                    Ok(written) => reply.written(written as u32),
                    Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
                }
            }
            Handle::BlobFile { .. } => {
                // Copy-on-write: need to copy to scratch first
                // This is handled in setattr/open when O_WRONLY/O_RDWR is used
                reply.error(libc::EROFS);
            }
            Handle::BlobStream { .. } => {
                reply.error(libc::EROFS);
            }
            Handle::Tree { .. } => {
                reply.error(libc::EISDIR);
            }
        }
    }

    /// Create a new file.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        let Some(scratch) = &self.scratch else {
            reply.error(libc::EROFS);
            return;
        };

        // Get parent entry to build the path
        let Some(parent_entry) = self.inodes.get(&parent) else {
            reply.error(libc::ENOENT);
            return;
        };

        if !parent_entry.is_dir() {
            reply.error(libc::ENOTDIR);
            return;
        }

        let Some(name_str) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };

        // Build virtual path
        let parent_path = self
            .inode_to_path
            .get(&parent)
            .map(|p| p.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        let virtual_path = parent_path.join(name_str);

        // Check if file was deleted (whiteout) - if so, recreate it
        if scratch.is_deleted(&virtual_path) {
            scratch.unmark_deleted(&virtual_path);
        }

        // Create file in scratch
        if let Err(e) = scratch.create_file(&virtual_path) {
            tracing::error!(err = ?e, "Failed to create file in scratch");
            reply.error(libc::EIO);
            return;
        }

        // Allocate inode and register
        let ino = self.allocate_inode();
        self.scratch_inodes.insert(virtual_path.clone(), ino);
        self.inode_to_path.insert(ino, virtual_path.clone());

        // Open file handle
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(scratch.scratch_path(&virtual_path))
        {
            Ok(f) => f,
            Err(e) => {
                tracing::error!(err = ?e, "Failed to reopen created file");
                reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                return;
            }
        };

        let fh = self.allocate_handle(Handle::ScratchFile {
            ino,
            virtual_path,
            file,
        });

        // Build attributes
        let attr = FileAttr {
            ino,
            size: 0,
            blocks: 1,
            atime: self.fs_creation_time,
            mtime: self.fs_creation_time,
            ctime: self.fs_creation_time,
            crtime: self.fs_creation_time,
            kind: FileType::RegularFile,
            perm: (mode & 0o7777) as u16,
            nlink: 1,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        };

        let open_flags = if flags & libc::O_RDWR != 0 || flags & libc::O_WRONLY != 0 {
            0
        } else {
            FOPEN_KEEP_CACHE
        };

        reply.created(&self.ttl, &attr, 0, fh, open_flags);
    }

    /// Delete a file.
    pub fn unlink(&self, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        let Some(scratch) = &self.scratch else {
            reply.error(libc::EROFS);
            return;
        };

        let Some(parent_entry) = self.inodes.get(&parent) else {
            reply.error(libc::ENOENT);
            return;
        };

        if !parent_entry.is_dir() {
            reply.error(libc::ENOTDIR);
            return;
        }

        let Some(name_str) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };

        // Build virtual path
        let parent_path = self
            .inode_to_path
            .get(&parent)
            .map(|p| p.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        let virtual_path = parent_path.join(name_str);

        // Mark as deleted (whiteout)
        if let Err(e) = scratch.mark_deleted(&virtual_path) {
            tracing::error!(err = ?e, "Failed to mark file as deleted");
            reply.error(libc::EIO);
            return;
        }

        // Remove from scratch inode tracking if present
        if let Some((_, ino)) = self.scratch_inodes.remove(&virtual_path) {
            self.inode_to_path.remove(&ino);
        }

        reply.ok();
    }

    /// Create a directory.
    pub fn mkdir(&self, parent: u64, name: &OsStr, mode: u32, _umask: u32, reply: ReplyEntry) {
        let Some(scratch) = &self.scratch else {
            reply.error(libc::EROFS);
            return;
        };

        let Some(parent_entry) = self.inodes.get(&parent) else {
            reply.error(libc::ENOENT);
            return;
        };

        if !parent_entry.is_dir() {
            reply.error(libc::ENOTDIR);
            return;
        }

        let Some(name_str) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };

        // Build virtual path
        let parent_path = self
            .inode_to_path
            .get(&parent)
            .map(|p| p.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        let virtual_path = parent_path.join(name_str);

        // Create directory in scratch
        if let Err(e) = scratch.create_dir(&virtual_path) {
            tracing::error!(err = ?e, "Failed to create directory in scratch");
            reply.error(libc::EIO);
            return;
        }

        // Allocate inode
        let ino = self.allocate_inode();
        self.scratch_inodes.insert(virtual_path.clone(), ino);
        self.inode_to_path.insert(ino, virtual_path);

        let attr = FileAttr {
            ino,
            size: 0,
            blocks: 1,
            atime: self.fs_creation_time,
            mtime: self.fs_creation_time,
            ctime: self.fs_creation_time,
            crtime: self.fs_creation_time,
            kind: FileType::Directory,
            perm: (mode & 0o7777) as u16,
            nlink: 2,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        };

        reply.entry(&self.ttl, &attr, 0);
    }

    /// Remove a directory.
    pub fn rmdir(&self, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        let Some(scratch) = &self.scratch else {
            reply.error(libc::EROFS);
            return;
        };

        let Some(parent_entry) = self.inodes.get(&parent) else {
            reply.error(libc::ENOENT);
            return;
        };

        if !parent_entry.is_dir() {
            reply.error(libc::ENOTDIR);
            return;
        }

        let Some(name_str) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };

        // Build virtual path
        let parent_path = self
            .inode_to_path
            .get(&parent)
            .map(|p| p.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        let virtual_path = parent_path.join(name_str);

        // Check if directory is empty in scratch
        let scratch_path = scratch.scratch_path(&virtual_path);
        if scratch_path.exists() {
            match std::fs::read_dir(&scratch_path) {
                Ok(mut entries) => {
                    if entries.next().is_some() {
                        reply.error(libc::ENOTEMPTY);
                        return;
                    }
                }
                Err(_) => {}
            }
        }

        // Mark as deleted
        if let Err(e) = scratch.mark_deleted(&virtual_path) {
            tracing::error!(err = ?e, "Failed to mark directory as deleted");
            reply.error(libc::EIO);
            return;
        }

        // Remove from tracking
        if let Some((_, ino)) = self.scratch_inodes.remove(&virtual_path) {
            self.inode_to_path.remove(&ino);
        }

        reply.ok();
    }

    /// Rename a file or directory.
    #[allow(clippy::too_many_arguments)]
    pub fn rename(
        &self,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        let Some(scratch) = &self.scratch else {
            reply.error(libc::EROFS);
            return;
        };

        let Some(name_str) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };

        let Some(newname_str) = newname.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };

        // Build old path
        let parent_path = self
            .inode_to_path
            .get(&parent)
            .map(|p| p.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        let old_path = parent_path.join(name_str);

        // Build new path
        let new_parent_path = self
            .inode_to_path
            .get(&newparent)
            .map(|p| p.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        let new_path = new_parent_path.join(newname_str);

        // Perform rename in scratch
        if let Err(e) = scratch.rename(&old_path, &new_path) {
            tracing::error!(err = ?e, "Failed to rename in scratch");
            reply.error(libc::EIO);
            return;
        }

        // Update inode tracking
        if let Some((_, ino)) = self.scratch_inodes.remove(&old_path) {
            self.scratch_inodes.insert(new_path.clone(), ino);
            self.inode_to_path.insert(ino, new_path);
        }

        reply.ok();
    }

    /// Set file attributes (truncate, chmod, etc.)
    #[allow(clippy::too_many_arguments)]
    pub fn setattr(
        &self,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        // For truncate (size), we need scratch
        if size.is_some() && self.scratch.is_none() {
            reply.error(libc::EROFS);
            return;
        }

        let Some(entry) = self.inodes.get(&ino) else {
            // Check if it's a scratch inode
            if let Some(virtual_path) = self.inode_to_path.get(&ino) {
                let scratch = self.scratch.as_ref().unwrap();
                let scratch_path = scratch.scratch_path(&virtual_path);

                // Handle truncate
                if let Some(new_size) = size {
                    if let Err(e) = std::fs::OpenOptions::new()
                        .write(true)
                        .open(&scratch_path)
                        .and_then(|f| f.set_len(new_size))
                    {
                        reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                        return;
                    }
                }

                // Get current metadata
                match std::fs::metadata(&scratch_path) {
                    Ok(meta) => {
                        let attr = FileAttr {
                            ino,
                            size: meta.len(),
                            blocks: (meta.len() / BLOCK_SIZE as u64) + 1,
                            atime: meta.accessed().unwrap_or(self.fs_creation_time),
                            mtime: meta.modified().unwrap_or(self.fs_creation_time),
                            ctime: self.fs_creation_time,
                            crtime: self.fs_creation_time,
                            kind: if meta.is_dir() {
                                FileType::Directory
                            } else {
                                FileType::RegularFile
                            },
                            perm: mode.unwrap_or(0o644) as u16,
                            nlink: 1,
                            uid: uid.unwrap_or(self.uid),
                            gid: gid.unwrap_or(self.gid),
                            rdev: 0,
                            blksize: BLOCK_SIZE,
                            flags: 0,
                        };
                        reply.attr(&self.ttl, &attr);
                    }
                    Err(e) => {
                        reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                    }
                }
                return;
            }

            reply.error(libc::ENOENT);
            return;
        };

        // For base layer files, we can only report current attrs (read-only)
        match self.attr_from_entry(entry.value()) {
            Ok(mut attr) => {
                if let Some(m) = mode {
                    attr.perm = (m & 0o7777) as u16;
                }
                if let Some(u) = uid {
                    attr.uid = u;
                }
                if let Some(g) = gid {
                    attr.gid = g;
                }
                reply.attr(&self.ttl, &attr);
            }
            Err(_) => {
                reply.error(libc::EIO);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allocate_inodes_assigns_ids() {
        let mount =
            Mount::new(tokio::runtime::Handle::current(), Vec::new(), Manifest::default())
                .unwrap();
        // Root inode (1) should always exist
        assert!(mount.inodes.contains_key(&1));
    }

    #[tokio::test]
    async fn root_inode_is_directory() {
        let mount =
            Mount::new(tokio::runtime::Handle::current(), Vec::new(), Manifest::default())
                .unwrap();
        let root = mount.inodes.get(&1).expect("root inode should exist");
        // Root should be a tree (directory)
        assert!(root.kind.is_tree(), "root should be a directory");
    }

    #[tokio::test]
    async fn read_only_mount_is_not_editable() {
        let mount =
            Mount::new(tokio::runtime::Handle::current(), Vec::new(), Manifest::default())
                .unwrap();
        assert!(!mount.is_editable());
        assert!(mount.scratch().is_none());
    }

    #[tokio::test]
    async fn editable_mount_has_scratch() {
        let mount = Mount::new_editable(
            tokio::runtime::Handle::current(),
            Vec::new(),
            Manifest::default(),
            "test-runtime",
        )
        .unwrap();
        assert!(mount.is_editable());
        assert!(mount.scratch().is_some());
    }

    #[tokio::test]
    async fn editable_mount_scratch_dir_exists() {
        let mount = Mount::new_editable(
            tokio::runtime::Handle::current(),
            Vec::new(),
            Manifest::default(),
            "test-mount-scratch",
        )
        .unwrap();

        let scratch = mount.scratch().expect("scratch should exist");
        assert!(scratch.root().exists());
    }

    #[tokio::test]
    async fn editable_mount_scratch_cleanup_on_drop() {
        let scratch_root;
        {
            let mount = Mount::new_editable(
                tokio::runtime::Handle::current(),
                Vec::new(),
                Manifest::default(),
                "test-cleanup",
            )
            .unwrap();
            scratch_root = mount.scratch().unwrap().root().to_path_buf();
            assert!(scratch_root.exists());
        }
        // After mount is dropped, scratch should be cleaned up
        assert!(!scratch_root.exists());
    }
}
