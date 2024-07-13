// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

pub type MigrationResult<T> = std::result::Result<T, MigrationError>;

#[derive(Debug, miette::Diagnostic, thiserror::Error)]
pub enum MigrationError {
    #[error("Repository path must have a file name")]
    NoFileName,

    #[error("Invalid repository root {1:?}")]
    InvalidRoot(std::path::PathBuf, #[source] std::io::Error),

    #[error("Error reading path {1:?} [{0}]")]
    ReadError(&'static str, std::path::PathBuf, #[source] std::io::Error),
    #[error("Error modifying path {1:?} [{0}]")]
    WriteError(&'static str, std::path::PathBuf, #[source] std::io::Error),

    /// Data already existing for this migration which
    /// makes it unsafe to continue the operation
    #[error("Found existing migration data: {0:?}")]
    ExistingData(std::path::PathBuf),

    #[error("Failed to parse repository version '{version}'")]
    InvalidVersion {
        version: String,
        source: semver::Error,
    },
}
