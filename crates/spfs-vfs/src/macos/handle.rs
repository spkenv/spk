// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! File handle types for macOS FUSE filesystem

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use spfs::tracking::{BlobRead, Entry};

/// A handle to a file or directory in the spfs runtime
pub enum Handle {
    /// A handle to a real file on disk that can be seek'd (read-only)
    BlobFile {
        /// The underlying entry data for this filesystem node
        entry: Arc<Entry<u64>>,
        /// The on-disk file containing this blob data
        file: std::fs::File,
    },
    /// A handle to an opaque file stream that can only be read once
    BlobStream {
        /// The underlying entry data for this filesystem node
        entry: Arc<Entry<u64>>,
        /// The current offset of the file stream
        ///
        /// Streams cannot be seek'd and must be read through contiguously
        /// and only once. This value is used to ensure that reads do not
        /// attempt to move the offset.
        offset: Arc<AtomicU64>,
        /// The opaque data stream for this blob
        stream: Arc<tokio::sync::Mutex<Pin<Box<dyn BlobRead>>>>,
    },
    /// A handle to an open directory that can be read
    Tree {
        /// The underlying entry data for this filesystem node
        entry: Arc<Entry<u64>>,
    },
    /// A handle to a file in the scratch directory (read-write)
    ///
    /// Used for editable mounts on macOS where files are copied
    /// to scratch on first write (copy-on-write semantics).
    ScratchFile {
        /// The allocated inode for this file
        ino: u64,
        /// The virtual path in the filesystem (e.g., "/bin/foo")
        virtual_path: PathBuf,
        /// The file handle open for read/write
        file: std::fs::File,
    },
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlobFile { entry, .. } => f
                .debug_struct("BlobFile")
                .field("ino", &entry.user_data)
                .finish(),
            Self::BlobStream { entry, .. } => f
                .debug_struct("BlobStream")
                .field("ino", &entry.user_data)
                .finish(),
            Self::Tree { entry } => f
                .debug_struct("Tree")
                .field("ino", &entry.user_data)
                .finish(),
            Self::ScratchFile { ino, virtual_path, .. } => f
                .debug_struct("ScratchFile")
                .field("ino", ino)
                .field("virtual_path", virtual_path)
                .finish(),
        }
    }
}

impl Handle {
    /// The allocated inode value for this handle
    pub fn ino(&self) -> u64 {
        match self {
            Self::BlobFile { entry, .. } => entry.user_data,
            Self::BlobStream { entry, .. } => entry.user_data,
            Self::Tree { entry } => entry.user_data,
            Self::ScratchFile { ino, .. } => *ino,
        }
    }

    /// An unowned reference to the entry data of this handle.
    ///
    /// # Panics
    /// Panics if called on a `ScratchFile` handle, which doesn't have an entry.
    pub fn entry(&self) -> &Entry<u64> {
        match self {
            Self::BlobFile { entry, .. } => entry,
            Self::BlobStream { entry, .. } => entry,
            Self::Tree { entry, .. } => entry,
            Self::ScratchFile { .. } => panic!("ScratchFile has no entry"),
        }
    }

    /// An owned reference to the entry data of this handle.
    ///
    /// # Panics
    /// Panics if called on a `ScratchFile` handle, which doesn't have an entry.
    pub fn entry_owned(&self) -> Arc<Entry<u64>> {
        match self {
            Self::BlobFile { entry, .. } => Arc::clone(entry),
            Self::BlobStream { entry, .. } => Arc::clone(entry),
            Self::Tree { entry, .. } => Arc::clone(entry),
            Self::ScratchFile { .. } => panic!("ScratchFile has no entry"),
        }
    }

    /// Returns true if this handle is for a directory
    pub fn is_dir(&self) -> bool {
        matches!(self, Self::Tree { .. })
    }

    /// Returns true if this is a scratch file handle (writable)
    pub fn is_scratch(&self) -> bool {
        matches!(self, Self::ScratchFile { .. })
    }

    /// Get the virtual path if this is a scratch file
    pub fn virtual_path(&self) -> Option<&PathBuf> {
        match self {
            Self::ScratchFile { virtual_path, .. } => Some(virtual_path),
            _ => None,
        }
    }
}
