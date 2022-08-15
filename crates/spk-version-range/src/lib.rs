// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
pub mod parsing;
mod version_range;

pub use error::{Error, Result};
pub use version_range::{
    parse_version_range, CompatRange, DoubleEqualsVersion, DoubleNotEqualsVersion, EqualsVersion,
    GreaterThanOrEqualToRange, GreaterThanRange, LessThanOrEqualToRange, LessThanRange,
    LowestSpecifiedRange, NotEqualsVersion, Ranged, RestrictMode, SemverRange, VersionFilter,
    VersionRange, WildcardRange, VERSION_RANGE_SEP,
};
