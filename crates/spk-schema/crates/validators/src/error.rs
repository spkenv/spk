// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema_ident::AnyIdent;
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
    VersionExistsError(AnyIdent),

    // Bake Errors
    #[error("Skip embedded")]
    SkipEmbedded,

    /// Not running under an active spk environment
    #[error("No current spfs runtime environment")]
    NoEnvironment,
}
