// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use relative_path::RelativePathBuf;
use variantly::Variantly;

#[derive(Variantly)]
pub enum TagPathStrategyType {
    /// Normalize the version in tag path.
    Normalized,
    /// Use the version as specified in the tag path.
    #[cfg(feature = "legacy-spk-version-tags")]
    Verbatim,
}

/// Specify what strategy to use for generating tag paths.
pub trait TagPathStrategy {
    fn strategy_type() -> TagPathStrategyType;
}

/// When creating a tag path that contains a version, this strategy will
/// normalize the version.
#[derive(Debug)]
pub struct NormalizedTagStrategy {}

impl TagPathStrategy for NormalizedTagStrategy {
    #[inline]
    fn strategy_type() -> TagPathStrategyType {
        TagPathStrategyType::Normalized
    }
}

/// When creating a tag path that contains a version, this strategy will
/// render the version as specified in the version object, without any
/// normalization.
#[cfg(feature = "legacy-spk-version-tags")]
#[derive(Debug)]
pub struct VerbatimTagStrategy {}

#[cfg(feature = "legacy-spk-version-tags")]
impl TagPathStrategy for VerbatimTagStrategy {
    #[inline]
    fn strategy_type() -> TagPathStrategyType {
        TagPathStrategyType::Verbatim
    }
}

pub trait TagPath {
    /// Return the relative path for the spfs tag for an ident.
    fn tag_path<S: TagPathStrategy>(&self) -> RelativePathBuf;
}
