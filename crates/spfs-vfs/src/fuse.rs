// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::io::{Seek, SeekFrom};
use std::mem::ManuallyDrop;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::prelude::FileExt;
#[cfg(feature = "fuse-backend-abi-7-31")]
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use dashmap::DashMap;
use fuser::consts::*;
use fuser::{
    FileAttr,
    FileType,
    MountOption,
    ReplyData,
    ReplyDirectory,
    ReplyDirectoryPlus,
    ReplyEntry,
    ReplyOpen,
    Request,
};
use spfs::storage::FromConfig;
#[cfg(feature = "fuse-backend-abi-7-31")]
use spfs::tracking::BlobRead;
use spfs::tracking::{Entry, EntryKind, EnvSpec, Manifest};
use spfs::OsError;
use tokio::io::AsyncReadExt;

/// Options to configure the FUSE filesystem and
/// its behavior at runtime
#[derive(Debug, Clone)]
pub struct Config {
    /// The permission bits for the root filesystem node
    pub root_mode: u32,
    /// The user id that should own all files and directories
    pub uid: nix::unistd::Uid,
    /// The group id that should own all files and directories
    pub gid: nix::unistd::Gid,
    /// Mount options to be used when setting up
    pub mount_options: HashSet<MountOption>,
    /// Remote repositories that can be read from.
    ///
    /// These are in addition to the local repository and
    /// are searched in order to find data.
    pub remotes: Vec<String>,
}

/// Handles the allocation of inodes, and async responses to all FUSE requests
struct Filesystem {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    opts: Config,

    ttl: Duration,
    next_inode: AtomicU64,
    next_handle: AtomicU64,
    inodes: DashMap<u64, Arc<Entry<u64>>>,
    handles: DashMap<u64, Handle>,
}

impl Filesystem {
    // establish a block site to report - this information
    // is not necessarily accurate, but because the filesystem
    // may or may not span any number of real disks we simply
    // report a realistic value for commands to use (eg du)
    const BLOCK_SIZE: u32 = 512;

    fn new(
        repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
        manifest: Manifest,
        opts: Config,
    ) -> Self {
        let fs = Self {
            repos,
            opts,
            ttl: Duration::from_secs(u64::MAX),
            // the root inode must be 1, which we are about to allocate
            next_inode: AtomicU64::new(1),
            // we do not allocate handle 0, so skip it for now
            next_handle: AtomicU64::new(1),
            inodes: Default::default(),
            handles: Default::default(),
        };
        // pre-allocate inodes for all entries in the manifest
        let mut root = manifest.take_root();
        // often manifests do not have appropriate mode bits set
        // at the root because they are not captured from the
        // actual directory upon commit. If we don't properly
        // report this mode as a directory, the kernel will
        // not like our FUSE filesystem.
        root.mode = fs.opts.root_mode | libc::S_IFDIR;
        fs.allocate_inodes(root);
        fs
    }

    fn allocate_handle_no(&self) -> u64 {
        self.next_handle.fetch_add(1, Ordering::Relaxed)
    }

    fn allocate_inode(&self) -> u64 {
        self.next_inode.fetch_add(1, Ordering::Relaxed)
    }

    fn allocate_inodes(&self, entry: Entry) -> Arc<Entry<u64>> {
        let Entry {
            kind,
            object,
            mode,
            size,
            entries,
            user_data: _,
        } = entry;

        let inode = self.allocate_inode();
        let entries = entries
            .into_iter()
            .map(|(n, e)| (n, self.allocate_inodes(e).as_ref().clone()))
            .collect();
        let entry = Arc::new(Entry {
            kind,
            object,
            mode,
            size,
            entries,
            user_data: inode,
        });
        self.inodes.insert(inode, Arc::clone(&entry));
        entry
    }

    fn allocate_handle(&self, data: Handle) -> u64 {
        loop {
            let id = self.allocate_handle_no();
            if id == 0 {
                // the 'empty/zero' handle value is never allocated
                // so that the explicit lack of handle can be detected in
                // function calls that take handles or inodes
                continue;
            }
            match self.handles.entry(id) {
                // continue until we find a vacant entry for this handle
                dashmap::mapref::entry::Entry::Occupied(_) => continue,
                dashmap::mapref::entry::Entry::Vacant(v) => {
                    v.insert(data);
                    break id;
                }
            }
        }
    }

    fn attr_from_entry(&self, entry: &Entry<u64>) -> FileAttr {
        let now = SystemTime::now();
        let kind = match entry.kind {
            EntryKind::Blob if entry.is_symlink() => FileType::Symlink,
            EntryKind::Blob => FileType::RegularFile,
            EntryKind::Tree => FileType::Directory,
            EntryKind::Mask => unreachable!(),
        };
        let size = if entry.is_dir() {
            entry.entries.len() as u64
        } else {
            entry.size
        };
        FileAttr {
            ino: entry.user_data,
            size,
            perm: entry.mode as u16, // truncate the non-perm bits
            uid: self.opts.uid.as_raw(),
            gid: self.opts.gid.as_raw(),
            blocks: (size / Self::BLOCK_SIZE as u64) + 1,
            atime: now,
            mtime: now,
            ctime: now,
            crtime: now,
            kind,
            // TODO: possibly return directory link count
            //       for all dirs below it (because of .. entries)
            nlink: if entry.is_dir() { 2 } else { 1 },
            rdev: 0,
            blksize: Self::BLOCK_SIZE,
            flags: 0,
        }
    }
}

/// Extract the ok value from a result, or reply with an error in FUSE
macro_rules! unwrap {
    ($reply:ident, $op:expr) => {{
        match $op {
            Ok(r) => r,
            Err(err) => err!($reply, err),
        }
    }};
}

/// Reply with an error to FUSE and return
macro_rules! err {
    ($reply:ident, $err:expr) => {{
        let err = $err;
        tracing::error!("{err:?}");
        let errno = err.os_error().unwrap_or(libc::EIO);
        $reply.error(errno);
        return;
    }};
}

// these functions mirror the actual fuse ones and
// so we don't have much control over the shape
#[allow(clippy::too_many_arguments)]
impl Filesystem {
    async fn statfs(&self, _ino: u64, reply: fuser::ReplyStatfs) {
        let blocks = self
            .inodes
            .iter()
            .map(|i| (i.value().size / Self::BLOCK_SIZE as u64) + 1)
            .sum();
        let files = self
            .inodes
            .iter()
            .filter(|i| i.value().kind.is_blob())
            .count();
        reply.statfs(
            blocks,
            0,
            0,
            files as u64,
            0,
            Self::BLOCK_SIZE,
            u32::MAX,
            Self::BLOCK_SIZE,
        )
    }

    async fn lookup(&self, parent: u64, name: OsString, reply: ReplyEntry) {
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };

        let Some(parent) = self.inodes.get(&parent) else {
            reply.error(libc::ENOENT);
            return;
        };

        tracing::trace!("lookup {name} in {}", parent.key());

        match parent.kind {
            EntryKind::Blob => {
                reply.error(libc::ENOTDIR);
                return;
            }
            EntryKind::Mask => {
                reply.error(libc::ENOENT);
                return;
            }
            EntryKind::Tree => {
                // entry.entries is reasonable to lookup into
            }
        }

        let Some(entry) = parent.entries.get(name) else {
            reply.error(libc::ENOENT);
            return;
        };

        let attr = self.attr_from_entry(entry);
        reply.entry(&self.ttl, &attr, 0);
    }

    async fn forget(&self, _ino: u64, _nlookup: u64) {
        // nothing to do, we never forget an inode because they are mapped
        // from the underlying manifest at startup
    }

    async fn getattr(&self, ino: u64, reply: fuser::ReplyAttr) {
        let Some(inode) = self.inodes.get(&ino) else {
            reply.error(libc::ENOENT);
            return;
        };

        let attr = self.attr_from_entry(inode.value());
        reply.attr(&self.ttl, &attr);
    }

    async fn readlink(&self, ino: u64, reply: ReplyData) {
        let Some(entry) = self.inodes.get(&ino).map(|kv| Arc::clone(kv.value())) else {
            reply.error(libc::ENOENT);
            return;
        };

        if !entry.is_symlink() {
            reply.error(libc::EINVAL);
            return;
        }

        let mut data = None;
        for repo in self.repos.iter() {
            match repo.open_payload(entry.object).await {
                Ok((mut reader, _)) => {
                    let mut bytes = Vec::new();
                    unwrap!(reply, reader.read_to_end(&mut bytes).await);
                    data = Some(bytes);
                    break;
                }
                Err(spfs::Error::UnknownObject(_)) => continue,
                Err(err) => {
                    err!(reply, err);
                }
            }
        }
        let Some(data) = data else {
            err!(reply, spfs::Error::UnknownObject(entry.object));
        };
        reply.data(data.as_slice());
    }

    async fn open(&self, ino: u64, flags: i32, reply: ReplyOpen) {
        let Some(entry) = self.inodes.get(&ino).map(|kv| Arc::clone(kv.value())) else {
            tracing::debug!("open {ino} = ENOENT");
            reply.error(libc::ENOENT);
            return;
        };

        if flags & (libc::O_WRONLY | libc::O_RDWR) != 0 {
            // TODO: support creating files?
            tracing::debug!("open {flags} = EROFS");
            reply.error(libc::EROFS);
            return;
        }

        let digest = match entry.kind {
            spfs::tracking::EntryKind::Tree => {
                tracing::debug!("open {ino} = EISDIR");
                reply.error(libc::EISDIR);
                return;
            }
            EntryKind::Mask => {
                tracing::debug!("open {ino}|Mask = ENOENT");
                reply.error(libc::ENOENT);
                return;
            }
            EntryKind::Blob => &entry.object,
        };

        let mut handle = None;
        #[allow(unused_mut)]
        let mut flags = FOPEN_KEEP_CACHE;
        for repo in self.repos.iter() {
            match &**repo {
                spfs::storage::RepositoryHandle::FS(fs_repo) => {
                    let Ok(fs_repo) = fs_repo.opened().await else {
                        reply.error(libc::ENOENT);
                        return;
                    };
                    let payload_path = fs_repo.payloads.build_digest_path(digest);
                    match std::fs::OpenOptions::new().read(true).open(payload_path) {
                        Ok(file) => {
                            handle = Some(Handle::BlobFile { entry, file });
                            break;
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            continue;
                        }
                        Err(err) => err!(reply, err),
                    }
                }
                #[cfg(feature = "fuse-backend-abi-7-31")]
                repo => match repo.open_payload(*digest).await {
                    Ok((stream, _)) => {
                        // TODO: try to leverage the returned file path?
                        handle = Some(Handle::BlobStream {
                            entry,
                            stream: tokio::sync::Mutex::new(stream),
                        });
                        flags |= FOPEN_NONSEEKABLE | FOPEN_STREAM;
                        break;
                    }
                    Err(spfs::Error::UnknownObject(_)) => continue,
                    Err(err) => err!(reply, err),
                },
                #[cfg(not(feature = "fuse-backend-abi-7-31"))]
                repo => {
                    tracing::error!(
                        "Attempting to use unsupported repo type with fuse: {}",
                        repo.address(),
                    );
                    reply.error(libc::ECONNREFUSED);
                    return;
                }
            }
        }

        let Some(handle) = handle else {
            // all repos were tried but none had the file that we wanted
            reply.error(libc::ENOENT);
            return;
        };

        let fh = self.allocate_handle(handle);

        tracing::trace!("open {ino} = {fh}");
        reply.opened(fh, flags);
    }

    async fn read(
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
            tracing::debug!("read {fh} = EBADF");
            reply.error(libc::EBADF);
            return;
        };

        match handle.value() {
            Handle::Tree { .. } => {
                tracing::debug!("read {fh} = EISDIR");
                reply.error(libc::EISDIR);
            }
            Handle::BlobFile { entry: _, file } => {
                // Safety: the fd must be valid and open, which we know. We also
                // know that the file will live for the livetime of this function
                // and so can create a copy of it safely for use before that rather
                // than duplicating it or using some kind of lock
                let f = unsafe { std::fs::File::from_raw_fd(file.as_raw_fd()) };
                // file takes ownership of the handle, but we need to make sure
                // it is not closed since it's a copy of the File that remains alive
                let f = ManuallyDrop::new(f);

                let mut buf = vec![0; size as usize];
                let mut consumed = 0;
                while consumed < size as usize {
                    let count = unwrap!(
                        reply,
                        f.read_at(&mut buf[consumed..], consumed as u64 + offset as u64)
                    );
                    consumed += count;
                    if count == 0 {
                        // the end of the file has been reached
                        break;
                    }
                }
                tracing::trace!("read {fh} = {consumed}/{size} [FILE]");
                reply.data(&buf[..consumed]);
            }
            #[cfg(feature = "fuse-backend-abi-7-31")]
            Handle::BlobStream { entry: _, stream } => {
                let mut stream = stream.lock().await;
                let mut buf = vec![0; size as usize];
                let mut consumed = 0;
                while consumed < size as usize {
                    let count = unwrap!(reply, stream.read(&mut buf[consumed..]).await);
                    consumed += count;
                    if count == 0 {
                        // the end of the file has been reached
                        break;
                    }
                }
                tracing::trace!("read {fh} = {consumed}/{size} [STREAM]");
                reply.data(&buf[..consumed]);
            }
        };
    }

    async fn release(
        &self,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        // ignore flush because we don't support write operations
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        let Some((_, _handle)) = self.handles.remove(&fh) else {
            reply.error(libc::EBADF);
            return;
        };

        reply.ok();
    }

    async fn opendir(&self, ino: u64, _flags: i32, reply: ReplyOpen) {
        let Some(entry) = self.inodes.get(&ino).map(|e| Arc::clone(e.value())) else {
            reply.error(libc::ENOENT);
            return;
        };

        let handle = match entry.kind {
            EntryKind::Blob => {
                reply.error(libc::ENOTDIR);
                return;
            }
            EntryKind::Mask => {
                reply.error(libc::ENOENT);
                return;
            }
            EntryKind::Tree => Handle::Tree { entry },
        };

        let fh = self.allocate_handle(handle);
        tracing::trace!("opendir {ino} = {fh}");
        #[allow(unused_mut)]
        let mut flags = 0;
        #[cfg(feature = "fuse-backend-abi-7-28")]
        {
            flags |= FOPEN_CACHE_DIR;
        }
        reply.opened(fh, flags);
    }

    async fn readdir(&self, _ino: u64, fh: u64, offset: i64, mut reply: ReplyDirectory) {
        tracing::trace!("readdir try_get_handle {fh} [{_ino}]");
        let Some(entry) = self.handles.get(&fh).map(|h| h.value().entry_owned()) else {
            reply.error(libc::EBADF);
            return;
        };

        let mut remaining = entry.entries.iter();
        if offset != 0 {
            // inode numbers are used to set dir offsets, so if the kernel gave one
            // we must take all entries up to and including that number
            //
            // we don't assume any specific sorting of the inodes or the directory
            // entries, only that the entry.entries field will not be reordered
            let mut next = remaining.next();
            while let Some(n) = next.take() {
                if n.1.user_data != offset as u64 {
                    next = remaining.next();
                }
            }
        }
        for (name, entry) in remaining {
            let kind = match entry.kind {
                EntryKind::Blob if entry.is_symlink() => FileType::Symlink,
                EntryKind::Blob => FileType::RegularFile,
                EntryKind::Tree => FileType::Directory,
                EntryKind::Mask => continue,
            };
            let ino = entry.user_data;
            let next_offset = ino as i64;
            let buffer_full = reply.add(ino, next_offset, kind, name);
            if buffer_full {
                break;
            }
        }
        reply.ok();
    }

    async fn readdirplus(&self, _ino: u64, fh: u64, offset: i64, mut reply: ReplyDirectoryPlus) {
        tracing::trace!("readdirplus try_get_handle {fh} @{offset}");
        let Some(entry) = self.handles.get(&fh).map(|h| h.value().entry_owned()) else {
            reply.error(libc::EBADF);
            return;
        };

        let mut remaining = entry.entries.iter();
        if offset != 0 {
            // inode numbers are used to set dir offsets, so if the kernel gave one
            // we must take all entries up to and including that number
            //
            // we don't assume any specific sorting of the inodes or the directory
            // entries, only that the entry.entries field will not be reordered
            let mut next = remaining.next();
            while let Some(n) = next.take() {
                if n.1.user_data != offset as u64 {
                    next = remaining.next();
                }
            }
        }
        for (name, entry) in remaining {
            let ino = entry.user_data;
            let next_offset = ino as i64;
            let attr = self.attr_from_entry(entry);
            tracing::trace!("readdirplus add {name}");
            let buffer_full = reply.add(ino, next_offset, name, &self.ttl, &attr, 0);
            if buffer_full {
                break;
            }
        }
        reply.ok()
    }

    async fn releasedir(&self, _ino: u64, fh: u64, _flags: i32, reply: fuser::ReplyEmpty) {
        let Some((_, _handle)) = self.handles.remove(&fh) else {
            reply.error(libc::EBADF);
            return;
        };
        reply.ok()
    }

    async fn lseek(&self, _ino: u64, fh: u64, offset: i64, whence: i32, reply: fuser::ReplyLseek) {
        let Some(handle) = self.handles.get_mut(&fh) else {
            tracing::debug!("lseek {fh} = EBADF");
            reply.error(libc::EBADF);
            return;
        };

        let file = match handle.value() {
            Handle::Tree { .. } => {
                tracing::debug!("lseek {fh} = EISDIR");
                reply.error(libc::EISDIR);
                return;
            }
            Handle::BlobFile { entry: _, file } => file,
            #[cfg(feature = "fuse-backend-abi-7-31")]
            Handle::BlobStream { .. } => {
                tracing::warn!("FUSE should not allow seek calls on streams");
                tracing::debug!("lseek {fh} = EINVAL");
                reply.error(libc::EINVAL);
                return;
            }
        };

        let pos = match whence {
            libc::SEEK_CUR => SeekFrom::Current(offset),
            libc::SEEK_END => SeekFrom::End(offset),
            libc::SEEK_SET => SeekFrom::Start(offset as u64),

            // From linux man pages: In the
            // simplest implementation, a filesystem can support the operations
            // by making SEEK_HOLE always return the offset of the end of the
            // file, and making SEEK_DATA always return offset (i.e., even if
            // the location referred to by offset is a hole, it can be
            // considered to consist of data that is a sequence of zeros).
            libc::SEEK_HOLE => SeekFrom::End(0),
            libc::SEEK_DATA => SeekFrom::Start(offset as u64),

            _ => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        // Safety: the fd must be valid and open, which we know. We also
        // know that the file will live for the livetime of this function
        // and so can create a copy of it safely for use before that rather
        // than duplicating it or using some kind of lock
        let f = unsafe { std::fs::File::from_raw_fd(file.as_raw_fd()) };
        // file takes ownership of the handle, but we need to make sure
        // it is not closed since it's a copy of the File that remains alive
        let mut f = ManuallyDrop::new(f);
        let new_offset = unwrap!(reply, f.seek(pos));
        reply.offset(new_offset as i64);
    }
}

/// Represents a connected FUSE session.
///
/// This implements the [`fuser::Filesystem`] trait, receives
/// all requests and arranges for their async execution in the
/// spfs virtual filesystem.
pub struct Session {
    inner: Arc<SessionInner>,
}

impl Session {
    /// Construct a new session which serves the provided reference
    /// in its filesystem
    pub fn new(reference: EnvSpec, opts: Config) -> Self {
        Self {
            inner: Arc::new(SessionInner {
                opts,
                reference,
                fs: tokio::sync::OnceCell::new(),
            }),
        }
    }
}

struct SessionInner {
    opts: Config,
    reference: EnvSpec,
    fs: tokio::sync::OnceCell<Arc<Filesystem>>,
}

impl SessionInner {
    async fn get_fs(&self) -> spfs::Result<Arc<Filesystem>> {
        self.fs
            .get_or_try_init(|| async {
                let config = spfs::Config::current()?;
                tracing::debug!("Opening repositories...");
                let proxy_config = spfs::storage::proxy::Config {
                    primary: config.storage.root.to_string_lossy().to_string(),
                    secondary: self.opts.remotes.clone(),
                };
                let repo = spfs::storage::ProxyRepository::from_config(proxy_config)
                    .await?
                    .into();

                tracing::debug!("Computing environment manifest...");
                let manifest = spfs::compute_environment_manifest(&self.reference, &repo).await?;

                let spfs::storage::RepositoryHandle::Proxy(repo) = repo else {
                    unreachable!();
                };

                let repos = repo.into_stack().into_iter().map(Arc::new).collect();
                Ok(Arc::new(Filesystem::new(
                    repos,
                    manifest,
                    self.opts.clone(),
                )))
            })
            .await
            .map(Arc::clone)
    }
}

impl fuser::Filesystem for Session {
    fn init(
        &mut self,
        _req: &Request<'_>,
        config: &mut fuser::KernelConfig,
    ) -> std::result::Result<(), libc::c_int> {
        const DESIRED: &[(&str, u32)] = &[
            ("FUSE_ASYNC_READ", FUSE_ASYNC_READ),
            ("FUSE_BIG_WRITES", FUSE_BIG_WRITES),
            ("FUSE_DO_READDIRPLUS", FUSE_DO_READDIRPLUS),
            ("FUSE_EXPORT_SUPPORT", FUSE_EXPORT_SUPPORT),
            ("FUSE_FILE_OPS", FUSE_FILE_OPS),
            ("FUSE_READDIRPLUS_AUTO", FUSE_READDIRPLUS_AUTO),
            #[cfg(feature = "fuse-backend-abi-7-25")]
            ("FUSE_PARALLEL_DIROPS", FUSE_PARALLEL_DIROPS),
            #[cfg(feature = "fuse-backend-abi-7-28")]
            ("FUSE_CACHE_SYMLINKS", FUSE_CACHE_SYMLINKS),
        ];
        let all_desired = DESIRED.iter().fold(0, |prev, (_, i)| prev | i);
        if let Err(unsupported) = config.add_capabilities(all_desired) {
            let rejected = DESIRED
                .iter()
                .filter_map(|d| (d.1 & unsupported != 0).then_some(d.0));
            for name in rejected {
                tracing::warn!("FUSE feature rejected: {name}");
            }
            config
                .add_capabilities(all_desired & !unsupported)
                .expect("should accept after we remove unsupported caps");
        }
        tracing::info!("Filesystem initialized");
        Ok(())
    }

    fn statfs(&mut self, _req: &Request<'_>, ino: u64, reply: fuser::ReplyStatfs) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.statfs(ino, reply).await
        });
    }

    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name = name.to_owned();
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.lookup(parent, name, reply).await
        });
    }

    fn forget(&mut self, _req: &Request<'_>, ino: u64, nlookup: u64) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            // if the once cell was never initialized then we shouldn't
            // have anything to forget
            if let Ok(fs) = session.get_fs().await {
                fs.forget(ino, nlookup).await
            }
        });
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, reply: fuser::ReplyAttr) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.getattr(ino, reply).await
        });
    }

    fn readlink(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyData) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.readlink(ino, reply).await
        });
    }

    fn open(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.open(ino, flags, reply).await
        });
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.read(ino, fh, offset, size, flags, lock_owner, reply)
                .await
        });
    }

    fn release(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        flags: i32,
        lock_owner: Option<u64>,
        flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.release(ino, fh, flags, lock_owner, flush, reply).await
        });
    }

    fn opendir(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.opendir(ino, flags, reply).await
        });
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: ReplyDirectory,
    ) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.readdir(ino, fh, offset, reply).await
        });
    }

    fn readdirplus(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: ReplyDirectoryPlus,
    ) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.readdirplus(ino, fh, offset, reply).await
        });
    }

    fn releasedir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        flags: i32,
        reply: fuser::ReplyEmpty,
    ) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.releasedir(ino, fh, flags, reply).await
        });
    }

    fn lseek(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        whence: i32,
        reply: fuser::ReplyLseek,
    ) {
        let session = Arc::clone(&self.inner);
        tokio::task::spawn(async move {
            let fs = unwrap!(reply, session.get_fs().await);
            fs.lseek(ino, fh, offset, whence, reply).await
        });
    }
}

enum Handle {
    /// A handle to real file on disk that can be seek'd, etc.
    BlobFile {
        entry: Arc<Entry<u64>>,
        file: std::fs::File,
    },
    #[cfg(feature = "fuse-backend-abi-7-31")]
    // A handle to an opaque file stream that can only be read once
    BlobStream {
        entry: Arc<Entry<u64>>,
        // TODO: we should avoid the tokio mutex at all costs,
        // but we need a mutable reference to this BlobRead and
        // need to hold it across an await (for reading from the stream)
        stream: tokio::sync::Mutex<Pin<Box<dyn BlobRead>>>,
    },
    Tree {
        entry: Arc<Entry<u64>>,
    },
}

impl Handle {
    fn entry_owned(&self) -> Arc<Entry<u64>> {
        match self {
            Self::BlobFile { entry, .. } => Arc::clone(entry),
            #[cfg(feature = "fuse-backend-abi-7-31")]
            Self::BlobStream { entry, .. } => Arc::clone(entry),
            Self::Tree { entry } => Arc::clone(entry),
        }
    }
}
