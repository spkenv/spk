// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use relative_path::RelativePathBuf;
use spfs::tracking::DiffMode;
use spk_schema_ident::{AnyIdent, VersionIdent};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Serde(#[from] serde_yaml::Error),
    #[error("Error: {0}")]
    String(String),

    // API Errors
    #[error(transparent)]
    InvalidVersionError(#[from] spk_schema_foundation::version::InvalidVersionError),
    #[error(transparent)]
    InvalidNameError(#[from] spk_schema_foundation::name::InvalidNameError),
    #[error(transparent)]
    InvalidBuildError(#[from] spk_schema_foundation::ident_build::InvalidBuildError),

    // Storage Errors
    #[error("Package not found: {0}")]
    PackageNotFoundError(AnyIdent),
    #[error("Version exists: {0}")]
    VersionExistsError(VersionIdent),

    // Bake Errors
    #[error("Skip embedded")]
    SkipEmbedded,

    /// Not running under an active spk environment
    #[error("No current spfs runtime environment")]
    NoEnvironment,

    // Validation Errors
    #[error("All generated files must be collected by a component. These ones were not: \n - {}", .0.join("\n - "))]
    SomeFilesNotCollected(Vec<String>),
    #[error("Build process created no files under {0:?}")]
    BuildMadeNoFilesToInstall(String),
    // Failing to Box this causes a clippy 'large_enum_variant' error in the solution::Error enum
    #[error("Existing file was {0:?}: {1:?}")]
    ExistingFileAltered(Box<DiffMode>, RelativePathBuf),
}
