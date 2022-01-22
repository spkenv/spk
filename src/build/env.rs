// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::{Path, PathBuf};

use crate::api;

#[cfg(test)]
#[path = "./env_test.rs"]
mod env_test;

/// Returns the directory that contains package metadata
///
/// This directory is included as part of the package itself, and
/// nearly always has a prefix of /spfs
pub fn data_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    prefix
        .as_ref()
        .join("spk")
        .join("pkg")
        .join(pkg.to_string())
}
