// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;
#[cfg(feature = "fuse-backend-abi-7-31")]
use std::collections::{HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::io::{Seek, SeekFrom};
use std::mem::ManuallyDrop;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::prelude::FileExt;
#[cfg(feature = "fuse-backend-abi-7-31")]
use std::pin::Pin;
use std::sync::Arc;
#[cfg(feature = "fuse-backend-abi-7-31")]
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
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
    ReplyIoctl,
    ReplyOpen,
    ReplyXattr,
    Request,
};
use spfs::OsError;
use spfs::prelude::*;
use spfs::storage::LocalRepository;
#[cfg(feature = "fuse-backend-abi-7-31")]
use spfs::tracking::BlobRead;
use spfs::tracking::{Entry, EntryKind, EnvSpec, Manifest};
use tokio::io::AsyncReadExt;

use crate::Error;

type Result<T> = std::result::Result<T, Error>;

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
    /// Whether to have the tags in the secondary repos included in
    /// the lookup methods.
    pub include_secondary_tags: bool,
    /// Maximum total bytes held in the in-memory remote blob cache.
    ///
    /// Remote blobs up to `blob_cache_max_single_bytes` in size are buffered
    /// in memory so they can be read and seeked at arbitrary offsets.
    pub blob_cache_max_bytes: usize,
    /// Maximum size of a single remote blob that will be buffered in memory.
    ///
    /// Blobs larger than this are instead downloaded once into the local
    /// repository so that future opens find them locally without a network
    /// round-trip, and so they can be managed by `spfs clean`.
    pub blob_cache_max_single_bytes: usize,
}

/// Byte-bounded in-memory cache for remote blob payloads.
///
/// Entries are evicted in insertion order (oldest first) once the total
/// byte count would exceed `max_bytes`.  Multiple open handles to the same
/// blob share the same `Arc<bytes::Bytes>` without copying.
#[cfg(feature = "fuse-backend-abi-7-31")]
struct BlobCache {
    /// Digest insertion order for eviction (front = oldest).
    order: VecDeque<spfs::encoding::Digest>,
    data: HashMap<spfs::encoding::Digest, Arc<bytes::Bytes>>,
    current_bytes: usize,
    max_bytes: usize,
}

#[cfg(feature = "fuse-backend-abi-7-31")]
impl BlobCache {
    fn new(max_bytes: usize) -> Self {
        Self {
            order: VecDeque::new(),
            data: HashMap::new(),
            current_bytes: 0,
            max_bytes,
        }
    }

    fn get(&self, digest: &spfs::encoding::Digest) -> Option<Arc<bytes::Bytes>> {
        self.data.get(digest).cloned()
    }

    /// Insert `bytes` under `digest`, evicting oldest entries to stay within
    /// `max_bytes`.  Returns a shared `Arc` to the stored slice.
    fn insert(&mut self, digest: spfs::encoding::Digest, bytes: bytes::Bytes) -> Arc<bytes::Bytes> {
        // A concurrent open may have already inserted this digest.
        if let Some(existing) = self.data.get(&digest) {
            return Arc::clone(existing);
        }
        let len = bytes.len();
        while !self.order.is_empty() && self.current_bytes + len > self.max_bytes {
            if let Some(evicted_digest) = self.order.pop_front()
                && let Some(evicted) = self.data.remove(&evicted_digest)
            {
                self.current_bytes = self.current_bytes.saturating_sub(evicted.len());
            }
        }
        let arc = Arc::new(bytes);
        self.data.insert(digest, Arc::clone(&arc));
        self.order.push_back(digest);
        self.current_bytes += len;
        arc
    }
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
    fs_creation_time: SystemTime,
    #[cfg(feature = "fuse-backend-abi-7-31")]
    blob_cache: Mutex<BlobCache>,
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
        #[cfg(feature = "fuse-backend-abi-7-31")]
        let blob_cache = Mutex::new(BlobCache::new(opts.blob_cache_max_bytes));
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
            fs_creation_time: SystemTime::now(),
            #[cfg(feature = "fuse-backend-abi-7-31")]
            blob_cache,
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
            entries,
            user_data: _,
            legacy_size,
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
            entries,
            user_data: inode,
            legacy_size,
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

    fn attr_from_entry(&self, entry: &Entry<u64>) -> Result<FileAttr> {
        let kind = match entry.kind {
            EntryKind::Blob(_) if entry.is_symlink() => FileType::Symlink,
            EntryKind::Blob(_) => FileType::RegularFile,
            EntryKind::Tree => FileType::Directory,
            EntryKind::Mask => return Err(Error::EntryIsMask),
        };
        let size = if entry.is_dir() {
            entry.entries.len() as u64
        } else {
            entry.size()
        };
        let nlink: u32 = if entry.is_dir() {
            // Directory has 2 hardlinks, one for . and one for the
            // entry in its parent (..), plus one for each
            // subdirectory. Symlinks do not count.
            (entry.entries.iter().filter(|(_n, e)| e.is_dir()).count() + 2) as u32
        } else {
            // Everything else just has itself
            1
        };

        Ok(FileAttr {
            ino: entry.user_data,
            size,
            perm: entry.mode as u16, // truncate the non-perm bits
            uid: self.opts.uid.as_raw(),
            gid: self.opts.gid.as_raw(),
            blocks: (size / Self::BLOCK_SIZE as u64) + 1,
            // Use the time of the filesystem creation as the times here so
            // that the filesystem appears to be static and unchanging.
            atime: self.fs_creation_time,
            mtime: self.fs_creation_time,
            ctime: self.fs_creation_time,
            crtime: self.fs_creation_time,
            kind,
            nlink,
            rdev: 0,
            blksize: Self::BLOCK_SIZE,
            flags: 0,
        })
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
            .map(|i| (i.value().size() / Self::BLOCK_SIZE as u64) + 1)
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
            EntryKind::Blob(_) => {
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

        let Ok(attr) = self.attr_from_entry(entry) else {
            reply.error(libc::ENOENT);
            return;
        };
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

        let Ok(attr) = self.attr_from_entry(inode.value()) else {
            reply.error(libc::ENOENT);
            return;
        };
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
                Err(err) if err.try_next_repo() => continue,
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
            EntryKind::Blob(_) => &entry.object,
        };

        let mut handle = None;
        #[allow(unused_mut)]
        let mut flags = FOPEN_KEEP_CACHE;

        // Fast path: serve from the in-memory blob cache without touching any
        // repository.  This is checked before the repo loop so that even the
        // FS repo open() syscall is avoided on a cache hit.
        #[cfg(feature = "fuse-backend-abi-7-31")]
        {
            let cached = self
                .blob_cache
                .lock()
                .expect("blob cache lock poisoned")
                .get(digest);
            if let Some(data) = cached {
                let fh = self.allocate_handle(Handle::BlobCached {
                    entry,
                    data,
                    current_offset: AtomicU64::new(0),
                });
                tracing::trace!("open {ino} = {fh} [CACHE HIT]");
                reply.opened(fh, FOPEN_KEEP_CACHE);
                return;
            }
        }

        for repo in self.repos.iter() {
            match &**repo {
                spfs::storage::RepositoryHandle::FS(fs_repo) => {
                    let Ok(fs_repo) = fs_repo.opened().await else {
                        reply.error(libc::ENOENT);
                        return;
                    };
                    let payload_path = fs_repo.payloads().build_digest_path(digest);
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
                    Ok((mut stream, _)) => {
                        let file_size = entry.size() as usize;
                        if file_size <= self.opts.blob_cache_max_single_bytes {
                            // Small enough to buffer in memory.  Download the
                            // full payload and add it to the shared LRU cache.
                            let mut buf = Vec::with_capacity(file_size);
                            unwrap!(reply, stream.read_to_end(&mut buf).await);
                            let data = self
                                .blob_cache
                                .lock()
                                .expect("blob cache lock poisoned")
                                .insert(*digest, bytes::Bytes::from(buf));
                            handle = Some(Handle::BlobCached {
                                entry,
                                data,
                                current_offset: AtomicU64::new(0),
                            });
                        } else {
                            // Too large for the in-memory cache.  Stream
                            // directly from the remote; the first
                            // non-sequential read or seek triggers a one-time
                            // download into the local FS repository.
                            let owned_digest = *digest;
                            handle = Some(Handle::BlobRemote {
                                entry,
                                digest: owned_digest,
                                inner: tokio::sync::Mutex::new(BlobRemoteInner::Streaming {
                                    stream,
                                    stream_pos: 0,
                                }),
                            });
                            flags = FOPEN_STREAM;
                        }
                        break;
                    }
                    Err(err) if err.try_next_repo() => continue,
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
            Handle::BlobCached { data, .. } => {
                let start = offset as usize;
                let end = (start + size as usize).min(data.len());
                let slice = if start < data.len() {
                    &data[start..end]
                } else {
                    &[]
                };
                tracing::trace!("read {fh} = {}/{size} [CACHED]", slice.len());
                reply.data(slice);
            }
            #[cfg(feature = "fuse-backend-abi-7-31")]
            Handle::BlobRemote {
                entry: _,
                digest,
                inner,
            } => {
                let mut guard = inner.lock().await;
                let read_offset = offset as u64;

                // Promote to a local file on the first non-sequential access.
                let needs_promotion = if let BlobRemoteInner::Streaming { stream_pos, .. } = &*guard
                {
                    *stream_pos != read_offset
                } else {
                    false
                };
                if needs_promotion {
                    tracing::debug!(
                        fh,
                        read_offset,
                        "non-sequential read on remote stream, promoting to local file"
                    );
                    unwrap!(
                        reply,
                        self.promote_remote_to_local(&mut guard, digest).await
                    );
                }

                match &mut *guard {
                    BlobRemoteInner::Streaming { stream, stream_pos } => {
                        let mut buf = vec![0; size as usize];
                        let mut consumed = 0;
                        while consumed < size as usize {
                            let count = unwrap!(reply, stream.read(&mut buf[consumed..]).await);
                            consumed += count;
                            if count == 0 {
                                break;
                            }
                        }
                        *stream_pos += consumed as u64;
                        tracing::trace!("read {fh} = {consumed}/{size} [REMOTE STREAM]");
                        reply.data(&buf[..consumed]);
                    }
                    BlobRemoteInner::Local(file) => {
                        let f = unsafe { std::fs::File::from_raw_fd(file.as_raw_fd()) };
                        let f = ManuallyDrop::new(f);
                        let mut buf = vec![0; size as usize];
                        let mut consumed = 0;
                        while consumed < size as usize {
                            let count = unwrap!(
                                reply,
                                f.read_at(&mut buf[consumed..], consumed as u64 + read_offset)
                            );
                            consumed += count;
                            if count == 0 {
                                break;
                            }
                        }
                        tracing::trace!("read {fh} = {consumed}/{size} [REMOTE LOCAL]");
                        reply.data(&buf[..consumed]);
                    }
                }
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
            EntryKind::Blob(_) => {
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
                EntryKind::Blob(_) if entry.is_symlink() => FileType::Symlink,
                EntryKind::Blob(_) => FileType::RegularFile,
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
            let Ok(attr) = self.attr_from_entry(entry) else {
                continue;
            };
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
        let Some(handle) = self.handles.get(&fh) else {
            tracing::debug!("lseek {fh} = EBADF");
            reply.error(libc::EBADF);
            return;
        };

        match handle.value() {
            Handle::Tree { .. } => {
                tracing::debug!("lseek {fh} = EISDIR");
                reply.error(libc::EISDIR);
            }
            #[cfg(feature = "fuse-backend-abi-7-31")]
            Handle::BlobCached {
                data,
                current_offset,
                ..
            } => {
                let cur = current_offset.load(Ordering::Relaxed);
                let file_len = data.len() as u64;
                let new_offset = match whence {
                    libc::SEEK_SET => offset as u64,
                    libc::SEEK_CUR => (cur as i64 + offset) as u64,
                    libc::SEEK_END => (file_len as i64 + offset) as u64,
                    // Simplest valid implementation per the linux man page:
                    // treat the entire file as data with no holes.
                    libc::SEEK_HOLE => file_len,
                    libc::SEEK_DATA => offset as u64,
                    _ => {
                        tracing::debug!("lseek {fh} = EINVAL");
                        reply.error(libc::EINVAL);
                        return;
                    }
                };
                current_offset.store(new_offset, Ordering::Relaxed);
                tracing::trace!("lseek {fh} = {new_offset} [CACHED]");
                reply.offset(new_offset as i64);
            }
            #[cfg(feature = "fuse-backend-abi-7-31")]
            Handle::BlobRemote {
                entry,
                digest,
                inner,
            } => {
                let file_len = entry.size();
                let mut guard = inner.lock().await;

                // For the Streaming state, handle trivial (non-position-
                // changing) seeks without touching the stream, and compute
                // a target position for all position-changing seeks so we can
                // promote before the borrow is held.
                let needs_promotion_to =
                    if let BlobRemoteInner::Streaming { stream_pos, .. } = &*guard {
                        // SEEK_HOLE / SEEK_DATA: answer without moving the stream.
                        if whence == libc::SEEK_HOLE {
                            tracing::trace!("lseek {fh} = {file_len} [REMOTE STREAM SEEK_HOLE]");
                            reply.offset(file_len as i64);
                            return;
                        }
                        if whence == libc::SEEK_DATA {
                            tracing::trace!("lseek {fh} = {offset} [REMOTE STREAM SEEK_DATA]");
                            reply.offset(offset);
                            return;
                        }
                        let new_pos = match whence {
                            libc::SEEK_SET => offset as u64,
                            libc::SEEK_CUR => (*stream_pos as i64 + offset) as u64,
                            libc::SEEK_END => (file_len as i64 + offset) as u64,
                            _ => {
                                tracing::debug!("lseek {fh} = EINVAL");
                                reply.error(libc::EINVAL);
                                return;
                            }
                        };
                        if new_pos == *stream_pos {
                            // No-op: position unchanged, stream is still valid.
                            tracing::trace!("lseek {fh} = {new_pos} [REMOTE STREAM noop]");
                            reply.offset(new_pos as i64);
                            return;
                        }
                        // Position is changing: we can't rewind the stream,
                        // so promote to a local file first.
                        Some(new_pos)
                    } else {
                        None
                    };

                if let Some(new_pos) = needs_promotion_to {
                    tracing::debug!(
                        fh,
                        new_pos,
                        "seek requires rewind on remote stream, promoting to local file"
                    );
                    unwrap!(
                        reply,
                        self.promote_remote_to_local(&mut guard, digest).await
                    );
                    // Promotion succeeded; guard is now Local — seek to target.
                    match &mut *guard {
                        BlobRemoteInner::Local(file) => {
                            let f = unsafe { std::fs::File::from_raw_fd(file.as_raw_fd()) };
                            let mut f = ManuallyDrop::new(f);
                            let new_offset = unwrap!(reply, f.seek(SeekFrom::Start(new_pos)));
                            tracing::trace!("lseek {fh} = {new_offset} [REMOTE PROMOTED]");
                            reply.offset(new_offset as i64);
                        }
                        BlobRemoteInner::Streaming { .. } => {
                            unreachable!("promote_remote_to_local must transition to Local state");
                        }
                    }
                    return;
                }

                // Guard is already Local (the Streaming branch above either
                // returned early or set needs_promotion_to, so reaching here
                // means guard is Local).
                match &mut *guard {
                    BlobRemoteInner::Local(file) => {
                        let pos = match whence {
                            libc::SEEK_CUR => SeekFrom::Current(offset),
                            libc::SEEK_END => SeekFrom::End(offset),
                            libc::SEEK_SET => SeekFrom::Start(offset as u64),
                            libc::SEEK_HOLE => SeekFrom::End(0),
                            libc::SEEK_DATA => SeekFrom::Start(offset as u64),
                            _ => {
                                tracing::debug!("lseek {fh} = EINVAL");
                                reply.error(libc::EINVAL);
                                return;
                            }
                        };
                        let f = unsafe { std::fs::File::from_raw_fd(file.as_raw_fd()) };
                        let mut f = ManuallyDrop::new(f);
                        let new_offset = unwrap!(reply, f.seek(pos));
                        tracing::trace!("lseek {fh} = {new_offset} [REMOTE LOCAL]");
                        reply.offset(new_offset as i64);
                    }
                    BlobRemoteInner::Streaming { .. } => {
                        unreachable!("Streaming state must have been handled above");
                    }
                }
            }
            Handle::BlobFile { entry: _, file } => {
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
                // know that the file will live for the lifetime of this function
                // and so can create a copy of it safely for use before that rather
                // than duplicating it or using some kind of lock.
                let f = unsafe { std::fs::File::from_raw_fd(file.as_raw_fd()) };
                // file takes ownership of the handle, but we need to make sure
                // it is not closed since it's a copy of the File that remains alive
                let mut f = ManuallyDrop::new(f);
                let new_offset = unwrap!(reply, f.seek(pos));
                tracing::trace!("lseek {fh} = {new_offset} [FILE]");
                reply.offset(new_offset as i64);
            }
        }
    }

    /// Download a large remote blob to the local FS repository and replace
    /// the streaming handle state with a `Local` file handle.
    ///
    /// Idempotent: if the blob is already on disk (either from a previous
    /// promotion or because the FS repo already had it), only the file open
    /// is performed.
    #[cfg(feature = "fuse-backend-abi-7-31")]
    async fn promote_remote_to_local(
        &self,
        guard: &mut tokio::sync::MutexGuard<'_, BlobRemoteInner>,
        digest: &spfs::encoding::Digest,
    ) -> spfs::Result<()> {
        if matches!(**guard, BlobRemoteInner::Local(_)) {
            return Ok(());
        }

        let local_fs = self.repos.iter().find_map(|r| {
            if let spfs::storage::RepositoryHandle::FS(fs) = &**r {
                Some(fs)
            } else {
                None
            }
        });
        let local_fs = local_fs.ok_or_else(|| {
            spfs::Error::String("no local FS repository available to cache large blob".into())
        })?;

        let opened = local_fs.opened().await?;
        let payload_path = opened.payloads().build_digest_path(digest);

        // Only download if the payload is not already present on disk.
        if std::fs::metadata(&payload_path).is_err() {
            tracing::debug!(%digest, "promoting remote blob to local FS repository");
            let mut fresh_stream = None;
            for repo in self.repos.iter() {
                if matches!(&**repo, spfs::storage::RepositoryHandle::FS(_)) {
                    continue;
                }
                match repo.open_payload(*digest).await {
                    Ok((s, _)) => {
                        fresh_stream = Some(s);
                        break;
                    }
                    Err(err) if err.try_next_repo() => continue,
                    Err(err) => return Err(err),
                }
            }
            let stream = fresh_stream.ok_or(spfs::Error::UnknownObject(*digest))?;
            let stored = opened.commit_blob(stream).await?;
            if stored != *digest {
                return Err(spfs::Error::String(format!(
                    "payload digest mismatch when caching blob: expected {digest}, got {stored}"
                )));
            }
        }

        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(&payload_path)
            .map_err(|e| spfs::Error::String(format!("failed to open cached payload: {e}")))?;
        **guard = BlobRemoteInner::Local(file);
        Ok(())
    }
}

/// Represents a connected FUSE session.
///
/// This implements the [`fuser::Filesystem`] trait, receives
/// all requests and arranges for their async execution in the
/// spfs virtual filesystem.
#[derive(Clone)]
pub struct Session {
    inner: Arc<SessionInner>,
}

impl Session {
    /// Construct a new session which serves the provided reference
    /// in its filesystem
    pub fn new(reference: EnvSpec, opts: Config) -> Self {
        let session_start = tokio::time::Instant::now();

        Self {
            inner: Arc::new(SessionInner {
                opts,
                reference,
                fs: tokio::sync::OnceCell::new(),
                session_start,
                last_heartbeat_seconds_since_session_start: AtomicU64::new(0),
            }),
        }
    }

    /// Return the number of seconds since the last heartbeat was received
    pub fn seconds_since_last_heartbeat(&self) -> u64 {
        self.inner.session_start.elapsed().as_secs()
            - self
                .inner
                .last_heartbeat_seconds_since_session_start
                .load(Ordering::Relaxed)
    }
}

struct SessionInner {
    opts: Config,
    reference: EnvSpec,
    fs: tokio::sync::OnceCell<Arc<Filesystem>>,
    session_start: tokio::time::Instant,
    last_heartbeat_seconds_since_session_start: AtomicU64,
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
                    include_secondary_tags: self.opts.include_secondary_tags,
                };
                let repo = spfs::storage::ProxyRepository::from_config(proxy_config)
                    .await
                    .map_err(|source| spfs::Error::FailedToOpenRepository {
                        repository: "<FUSE Repository Stack>".into(),
                        source,
                    })?
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
            .cloned()
    }
}

impl fuser::Filesystem for Session {
    fn init(
        &mut self,
        _req: &Request<'_>,
        config: &mut fuser::KernelConfig,
    ) -> std::result::Result<(), libc::c_int> {
        const DESIRED: &[(&str, u64)] = &[
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
        if name
            .as_bytes()
            .starts_with(spfs::config::Fuse::HEARTBEAT_FILENAME_PREFIX.as_bytes())
        {
            let seconds_since_session_start = self.inner.session_start.elapsed().as_secs();
            tracing::trace!(?seconds_since_session_start, "heard heartbeat");
            self.inner
                .last_heartbeat_seconds_since_session_start
                .store(seconds_since_session_start, Ordering::Relaxed);

            // The heartbeat filename is sufficiently unique that the reply can
            // be sent without doing any real I/O on the backing filesystem.
            reply.error(libc::ENOENT);
            return;
        }

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

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: fuser::ReplyAttr) {
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

    fn flush(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn access(&mut self, _req: &Request<'_>, _ino: u64, _mask: i32, reply: fuser::ReplyEmpty) {
        reply.error(libc::ENOSYS);
    }

    fn getxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &OsStr,
        _size: u32,
        reply: ReplyXattr,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn ioctl(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _flags: u32,
        _cmd: u32,
        _in_data: &[u8],
        _out_size: u32,
        reply: ReplyIoctl,
    ) {
        reply.error(libc::ENOSYS);
    }
}

/// State for a lazily-promoted large remote blob.
///
/// Starts as `Streaming` (sequential reads from the remote) and transitions to
/// `Local` on the first non-sequential read or meaningful lseek.
#[cfg(feature = "fuse-backend-abi-7-31")]
enum BlobRemoteInner {
    Streaming {
        stream: Pin<Box<dyn BlobRead>>,
        /// Byte offset of the next byte the stream will yield.
        stream_pos: u64,
    },
    /// The blob has been downloaded to the local FS repository and is
    /// accessible as a regular file.
    Local(std::fs::File),
}

enum Handle {
    /// A handle to real file on disk that can be seek'd, etc.
    BlobFile {
        entry: Arc<Entry<u64>>,
        file: std::fs::File,
    },
    #[cfg(feature = "fuse-backend-abi-7-31")]
    /// A remote blob fully buffered in memory, allowing arbitrary seeks and
    /// random-access reads without a "current position" in the FUSE sense
    /// (FUSE read() always supplies an explicit absolute offset).  The
    /// `current_offset` field is only maintained for SEEK_CUR lseek calls.
    BlobCached {
        entry: Arc<Entry<u64>>,
        data: Arc<bytes::Bytes>,
        current_offset: AtomicU64,
    },
    #[cfg(feature = "fuse-backend-abi-7-31")]
    /// A large remote blob served lazily: sequential reads stream directly
    /// from the remote; the first non-sequential read or meaningful seek
    /// promotes the blob to the local FS repository so all subsequent
    /// accesses are from disk.
    BlobRemote {
        entry: Arc<Entry<u64>>,
        digest: spfs::encoding::Digest,
        inner: tokio::sync::Mutex<BlobRemoteInner>,
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
            Self::BlobCached { entry, .. } => Arc::clone(entry),
            #[cfg(feature = "fuse-backend-abi-7-31")]
            Self::BlobRemote { entry, .. } => Arc::clone(entry),
            Self::Tree { entry } => Arc::clone(entry),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "fuse-backend-abi-7-31")]
mod tests {
    use std::sync::Arc;

    use spfs::tracking::Manifest;

    use super::*;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_config() -> Config {
        Config {
            root_mode: 0o755 | libc::S_IFDIR as u32,
            uid: nix::unistd::Uid::current(),
            gid: nix::unistd::Gid::current(),
            mount_options: Default::default(),
            remotes: vec![],
            include_secondary_tags: false,
            blob_cache_max_bytes: 64 * 1024 * 1024,
            blob_cache_max_single_bytes: 8 * 1024 * 1024,
        }
    }

    fn make_filesystem(repos: Vec<Arc<spfs::storage::RepositoryHandle>>) -> Filesystem {
        Filesystem::new(repos, Manifest::default(), make_config())
    }

    /// Convenience: create a `Digest` from a single repeated byte (for distinct
    /// test digests without computing real hashes).
    fn fake_digest(byte: u8) -> spfs::encoding::Digest {
        spfs::encoding::Digest::from_bytes(&[byte; spfs::encoding::DIGEST_SIZE]).unwrap()
    }

    // ── BlobCache ─────────────────────────────────────────────────────────────

    #[test]
    fn blob_cache_insert_and_get() {
        let mut cache = BlobCache::new(1024);
        let digest = fake_digest(1);
        let bytes = bytes::Bytes::from_static(b"hello cache");
        let stored = cache.insert(digest, bytes.clone());
        assert_eq!(*stored, bytes);
        let hit = cache.get(&digest).expect("digest should be cached");
        assert_eq!(*hit, bytes);
    }

    #[test]
    fn blob_cache_miss_on_empty() {
        let cache = BlobCache::new(1024);
        assert!(cache.get(&fake_digest(42)).is_none());
    }

    #[test]
    fn blob_cache_evicts_oldest_when_full() {
        // Cap at 20 bytes; each entry is 10 bytes, so the third insert must
        // evict the first.
        let mut cache = BlobCache::new(20);
        let (d1, d2, d3) = (fake_digest(1), fake_digest(2), fake_digest(3));
        cache.insert(d1, bytes::Bytes::from(vec![0u8; 10]));
        cache.insert(d2, bytes::Bytes::from(vec![0u8; 10]));
        cache.insert(d3, bytes::Bytes::from(vec![0u8; 10])); // evicts d1

        assert!(cache.get(&d1).is_none(), "oldest entry should be evicted");
        assert!(cache.get(&d2).is_some());
        assert!(cache.get(&d3).is_some());
    }

    #[test]
    fn blob_cache_duplicate_insert_returns_same_arc() {
        let mut cache = BlobCache::new(1024);
        let digest = fake_digest(5);
        let a1 = cache.insert(digest, bytes::Bytes::from_static(b"data"));
        let a2 = cache.insert(digest, bytes::Bytes::from_static(b"data"));
        assert!(
            Arc::ptr_eq(&a1, &a2),
            "duplicate insert should reuse the existing Arc"
        );
    }

    // ── promote_remote_to_local ───────────────────────────────────────────────

    /// Helper: commit `content` to a freshly created FS repo and return the
    /// repo together with the blob's digest.
    async fn create_repo_with_blob(
        path: &std::path::Path,
        content: &[u8],
    ) -> (
        spfs::storage::fs::MaybeOpenFsRepository,
        spfs::encoding::Digest,
    ) {
        let repo = spfs::storage::fs::MaybeOpenFsRepository::create(path)
            .await
            .unwrap();
        let stream: Pin<Box<dyn BlobRead>> = Box::pin(std::io::Cursor::new(content.to_vec()));
        let opened = repo.opened().await.unwrap();
        let digest = opened.commit_blob(stream).await.unwrap();
        drop(opened);
        (repo, digest)
    }

    #[tokio::test]
    async fn promote_uses_existing_on_disk_payload() {
        // If the blob is already present in the local FS repository (e.g. from
        // a prior mount), promote_remote_to_local must succeed without
        // attempting a remote download and must transition the guard to Local.
        let tmpdir = tempfile::tempdir().unwrap();
        let content = b"payload for promotion test";
        let (repo, digest) = create_repo_with_blob(tmpdir.path(), content).await;

        let repos = vec![Arc::new(spfs::storage::RepositoryHandle::FS(repo))];
        let fs = make_filesystem(repos);

        let cursor: Pin<Box<dyn BlobRead>> = Box::pin(std::io::Cursor::new(content.to_vec()));
        let mutex = tokio::sync::Mutex::new(BlobRemoteInner::Streaming {
            stream: cursor,
            stream_pos: 0,
        });
        let mut guard = mutex.lock().await;

        fs.promote_remote_to_local(&mut guard, &digest)
            .await
            .expect("promotion should succeed when payload is already on disk");

        assert!(
            matches!(*guard, BlobRemoteInner::Local(_)),
            "guard should be Local after promotion"
        );
    }

    #[tokio::test]
    async fn promote_idempotent_when_already_local() {
        // A guard that is already in the Local state must be a no-op.
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("dummy");
        std::fs::write(&path, b"x").unwrap();
        let file = std::fs::File::open(&path).unwrap();

        let mutex = tokio::sync::Mutex::new(BlobRemoteInner::Local(file));
        let mut guard = mutex.lock().await;

        // Repos list is empty — if promote tried to do real work it would error.
        let fs = make_filesystem(vec![]);
        let digest = fake_digest(7);

        fs.promote_remote_to_local(&mut guard, &digest)
            .await
            .expect("promote on already-Local handle should succeed");
        assert!(matches!(*guard, BlobRemoteInner::Local(_)));
    }

    #[tokio::test]
    async fn promote_fails_without_local_fs_repo() {
        // Without any FS repository in the stack, promotion cannot write the
        // blob and must return an error rather than panic.
        let fs = make_filesystem(vec![]);
        let digest = fake_digest(8);

        let cursor: Pin<Box<dyn BlobRead>> = Box::pin(std::io::Cursor::new(vec![]));
        let mutex = tokio::sync::Mutex::new(BlobRemoteInner::Streaming {
            stream: cursor,
            stream_pos: 0,
        });
        let mut guard = mutex.lock().await;

        assert!(
            fs.promote_remote_to_local(&mut guard, &digest)
                .await
                .is_err(),
            "promotion should fail when no local FS repository is configured"
        );
    }
}
