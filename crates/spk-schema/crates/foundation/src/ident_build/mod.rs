// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod build;
mod build_id;
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
pub use build_id::BuildId;
pub use error::{Error, Result};
