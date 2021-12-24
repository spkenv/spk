// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use relative_path::RelativePathBuf;

use crate::api;

#[cfg(test)]
#[path = "./env_test.rs"]
mod env_test;

/// Returns the directory that contains package metadata
///
/// This directory is included as part of the package itself, and
/// should nearly always be expected to be relative to /spfs
pub fn data_path(pkg: &api::Ident) -> RelativePathBuf {
    RelativePathBuf::from("spk/pkg").join(pkg.to_string())
}
