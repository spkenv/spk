// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod build_key;
mod error;
mod package_iterator;
mod promotion_patterns;

pub use build_key::BuildKey;
pub use error::{Error, Result};
pub use package_iterator::{
    BUILD_SORT_TARGET,
    BuildIterator,
    BuildToSortedOptName,
    EmptyBuildIterator,
    PackageIterator,
    RepositoryPackageIterator,
    SortedBuildIterator,
};
pub use promotion_patterns::PromotionPatterns;
