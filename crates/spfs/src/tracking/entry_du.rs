// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::Stream;

use crate::{encoding, Result};

pub const LEVEL_SEPARATOR: char = '/';

pub trait DiskUsage {
    fn walk(&self) -> Pin<Box<dyn Stream<Item = Result<EntryDiskUsage>> + Send + Sync + '_>>;
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct EntryDiskUsage {
    path: Vec<String>,
    size: u64,
    digest: encoding::Digest,
}

impl EntryDiskUsage {
    pub fn new(path: Vec<String>, size: u64, digest: encoding::Digest) -> Self {
        Self { path, size, digest }
    }

    pub fn path(&self) -> String {
        self.path.join(&LEVEL_SEPARATOR.to_string())
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn digest(&self) -> &encoding::Digest {
        &self.digest
    }
}
