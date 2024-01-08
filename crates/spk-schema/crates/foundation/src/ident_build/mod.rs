// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build;
mod digest;
mod error;
mod format;
pub mod parsing;

pub use build::{
    parse_build,
    Build,
    EmbeddedSource,
    EmbeddedSourcePackage,
    InvalidBuildError,
    EMBEDDED,
    SRC,
};
pub use digest::Digest;
pub use error::{Error, Result};
