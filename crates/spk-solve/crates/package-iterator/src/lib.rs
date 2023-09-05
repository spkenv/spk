// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod build_key;
mod error;
mod package_iterator;
mod promotion_patterns;

pub use error::{Error, Result};
pub use package_iterator::{
    BuildIterator,
    EmptyBuildIterator,
    PackageIterator,
    RepositoryPackageIterator,
    SortedBuildIterator,
    BUILD_SORT_TARGET,
};
pub use promotion_patterns::PromotionPatterns;
