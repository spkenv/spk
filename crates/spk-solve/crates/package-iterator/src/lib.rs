// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build_key;
mod error;
mod package_iterator;

pub use error::{Error, Result};
pub use package_iterator::{
    BuildIterator,
    EmptyBuildIterator,
    PackageIterator,
    RepositoryPackageIterator,
    SortedBuildIterator,
    BUILD_SORT_TARGET,
};
