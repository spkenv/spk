// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build;
mod error;

#[cfg(test)]
#[path = "./archive_test.rs"]
mod archive_test;

pub use build::{source_package_path, BinaryPackageBuilder, BuildSource, SourcePackageBuilder};
pub use error::{Error, Result};
