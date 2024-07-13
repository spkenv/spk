// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use miette::Diagnostic;
use thiserror::Error;

#[cfg(test)]
#[path = "./error_test.rs"]
mod error_test;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Diagnostic, Debug, Error)]
#[diagnostic(
    url(
        "https://getspk.io/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spk::generic"))
    )
)]
pub enum Error {
    #[error("Failed to open file {0}")]
    FileOpenError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to write file {0}")]
    FileWriteError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Invalid inheritance: {0}")]
    InvalidInheritance(#[source] serde_yaml::Error),
    #[error("Invalid package spec file {0}: {1}")]
    InvalidPackageSpecFile(
        std::path::PathBuf,
        #[source] Box<format_serde_error::SerdeError>,
    ),
    #[error("Invalid path {0}")]
    InvalidPath(std::path::PathBuf, #[source] std::io::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    ProcessSpawnError(spfs::Error),
    #[error("Failed to wait for process: {0}")]
    ProcessWaitError(#[source] std::io::Error),
    #[error("Failed to encode spec: {0}")]
    SpecEncodingError(#[source] serde_yaml::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    Error(#[from] spfs::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentBuildError(#[from] crate::foundation::ident_build::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentComponentError(#[from] crate::foundation::ident_component::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentError(#[from] crate::ident::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkNameError(#[from] crate::foundation::name::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkOptionMapError(#[from] crate::foundation::option_map::Error),
    #[error("Error: {0}")]
    String(String),
    #[error("Failed to create temp dir: {0}")]
    TempDirError(#[source] std::io::Error),

    #[error(transparent)]
    InvalidYaml(#[from] format_serde_error::SerdeError),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    InvalidTemplate(#[from] spk_schema_tera::Error),

    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkConfigError(#[from] spk_config::Error),
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
