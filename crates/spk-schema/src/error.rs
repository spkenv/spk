// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};

use spk_schema_foundation::name::OptNameBuf;
use thiserror::Error;

#[cfg(test)]
#[path = "./error_test.rs"]
mod error_test;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to open file {0}")]
    FileOpenError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to write file {0}")]
    FileWriteError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Invalid inheritance: {0}")]
    InvalidInheritance(#[source] serde_yaml::Error),
    #[error("Invalid package spec file {0}: {1}")]
    InvalidPackageSpecFile(std::path::PathBuf, #[source] format_serde_error::SerdeError),
    #[error("Invalid path {0}")]
    InvalidPath(std::path::PathBuf, #[source] std::io::Error),
    #[error(transparent)]
    ProcessSpawnError(spfs::Error),
    #[error("Failed to wait for process: {0}")]
    ProcessWaitError(#[source] std::io::Error),
    #[error("Failed to encode spec: {0}")]
    SpecEncodingError(#[source] serde_yaml::Error),
    #[error(transparent)]
    SPFS(#[from] spfs::Error),
    #[error(transparent)]
    SpkIdentBuildError(#[from] crate::foundation::ident_build::Error),
    #[error(transparent)]
    SpkIdentComponentError(#[from] crate::foundation::ident_component::Error),
    #[error(transparent)]
    SpkIdentError(#[from] crate::ident::Error),
    #[error(transparent)]
    SpkNameError(#[from] crate::foundation::name::Error),
    #[error(transparent)]
    SpkOptionMapError(#[from] crate::foundation::option_map::Error),
    #[error("Error: {0}")]
    String(String),
    #[error("Failed to create temp dir: {0}")]
    TempDirError(#[source] std::io::Error),
    #[error(
        "Multiple different values resolved for the same option(s): {}",
        .resolved
            .iter()
            .map(|(name, values)| format!(" - {name} resolved to {values:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    )]
    MultipleOptionValuesResolved {
        resolved: HashMap<OptNameBuf, HashSet<String>>,
    },

    #[error(transparent)]
    InvalidYaml(#[from] format_serde_error::SerdeError),
}

impl Error {
    /// Wraps an error message with a prefix, creating a contextual but generic error
    pub fn wrap<S: AsRef<str>>(prefix: S, err: Self) -> Self {
        Error::String(format!("{}: {:?}", prefix.as_ref(), err))
    }

    /// Wraps an error message with a prefix, creating a contextual error
    pub fn wrap_io<S: AsRef<str>>(prefix: S, err: std::io::Error) -> Error {
        Error::String(format!("{}: {:?}", prefix.as_ref(), err))
    }
}
