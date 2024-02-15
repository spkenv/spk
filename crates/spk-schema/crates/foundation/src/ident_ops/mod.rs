// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod metadata_path;
pub mod parsing;
mod tag_path;

pub use metadata_path::MetadataPath;
#[cfg(feature = "legacy-spk-version-tags")]
pub use tag_path::VerbatimTagStrategy;
pub use tag_path::{NormalizedTagStrategy, TagPath, TagPathStrategy, TagPathStrategyType};
