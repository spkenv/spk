// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use relative_path::RelativePathBuf;
use variantly::Variantly;

#[derive(Variantly)]
pub enum TagPathStrategyType {
    /// Normalize the version in tag path.
    Normalized,
    /// Use the version as specified in the tag path.
    Verbatim,
}

/// Specify what strategy to use for generating tag paths.
pub trait TagPathStrategy: Clone + 'static {
    fn strategy_type() -> TagPathStrategyType;
}

/// When creating a tag path that contains a version, this strategy will
/// normalize the version.
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct VerbatimTagStrategy {}

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
