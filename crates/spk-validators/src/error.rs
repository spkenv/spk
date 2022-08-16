// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_ident::Ident;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    // #[error(transparent)]
    // SPFS(#[from] spfs::Error),
    #[error(transparent)]
    Serde(#[from] serde_yaml::Error),
    // #[error(transparent)]
    // Solve(#[from] crate::solve::Error),
    #[error("Error: {0}")]
    String(String),

    // API Errors
    #[error(transparent)]
    InvalidVersionError(#[from] spk_foundation::version::InvalidVersionError),
    #[error(transparent)]
    InvalidNameError(#[from] spk_name::InvalidNameError),
    #[error(transparent)]
    InvalidBuildError(#[from] spk_foundation::ident_build::InvalidBuildError),

    // Storage Errors
    #[error("Package not found: {0}")]
    PackageNotFoundError(Ident),
    #[error("Version exists: {0}")]
    VersionExistsError(Ident),

    // Build Errors
    // #[error(transparent)]
    // Collection(#[from] build::CollectionError),
    // #[error(transparent)]
    // Build(#[from] build::BuildError),

    // Bake Errors
    #[error("Skip embedded")]
    SkipEmbedded,

    // Test Errors
    // #[error(transparent)]
    // Test(#[from] test::TestError),
    /// Not running under an active spk environment
    #[error("No current spfs runtime environment")]
    NoEnvironment,
}
