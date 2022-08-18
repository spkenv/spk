// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::ident_ops::MetadataPath;
use relative_path::RelativePathBuf;

#[cfg(test)]
#[path = "./env_test.rs"]
mod env_test;

/// Returns the directory that contains package metadata
///
/// This directory is included as part of the package itself, and
/// should nearly always be assumed as relative to /spfs
pub fn data_path<I>(pkg: &I) -> RelativePathBuf
where
    I: MetadataPath,
{
    RelativePathBuf::from("/spk/pkg").join(pkg.metadata_path())
}
