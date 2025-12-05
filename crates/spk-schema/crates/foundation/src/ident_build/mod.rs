// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod build;
mod build_id;
mod error;
mod format;
pub mod parsing;

pub use build::{
    Build, EMBEDDED, EmbeddedSource, EmbeddedSourcePackage, InvalidBuildError, SRC, parse_build,
};
pub use build_id::BuildId;
pub use error::{Error, Result};
