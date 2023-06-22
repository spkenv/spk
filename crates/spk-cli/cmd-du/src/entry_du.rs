// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use futures::Stream;
use spfs::encoding::Digest;

pub const LEVEL_SEPARATOR: char = '/';

pub trait DiskUsage {
    fn walk(&self) -> Pin<Box<dyn Stream<Item = Result<EntryDiskUsage>> + Send + Sync + '_>>;
}

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
