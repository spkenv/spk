// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use spfs::encoding::Digest;

pub const LEVEL_SEPARATOR: char = '/';

/// Disk usage of a entry
#[derive(Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct EntryDiskUsage {
    path: Vec<Arc<String>>,
    size: u64,
    digest: Digest,
}

impl EntryDiskUsage {
    pub fn new(path: Vec<Arc<String>>, size: u64, digest: Digest) -> Self {
        Self { path, size, digest }
    }

    pub fn path(&self) -> &Vec<Arc<String>> {
        &self.path
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}
