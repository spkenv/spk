// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;
use std::sync::Arc;

use spfs::tracking::{BlobRead, Entry};
use winfsp::filesystem::DirBuffer;

/// A handle to a file or directory in the spfs runtime
pub enum Handle {
    /// A handle to real file on disk that can be seek'd, etc.
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
        /// The opaque data stream for this blob
        // TODO: we should avoid the tokio mutex at all costs,
        // but we need a mutable reference to this BlobRead and
        // need to hold it across an await (for reading from the stream)
        stream: tokio::sync::Mutex<Pin<Box<dyn BlobRead>>>,
    },
    /// A handle to an open directory that can be read
    Tree {
        /// The underlying entry data for this filesystem node
        entry: Arc<Entry<u64>>,
        /// The winfsp-formatted directory information for this entry
        dir_buffer: DirBuffer,
    },
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle").finish_non_exhaustive()
    }
}

impl Handle {
    /// The allocated inode value for this handle
    pub fn ino(&self) -> u64 {
        match self {
            Self::BlobFile { entry, .. } => entry.user_data,
            Self::BlobStream { entry, .. } => entry.user_data,
            Self::Tree { entry, .. } => entry.user_data,
        }
    }

    /// An owned reference to the entry data of this handle
    pub fn entry_owned(&self) -> Arc<Entry<u64>> {
        match self {
            Self::BlobFile { entry, .. } => Arc::clone(entry),
            Self::BlobStream { entry, .. } => Arc::clone(entry),
            Self::Tree { entry, .. } => Arc::clone(entry),
        }
    }
}
