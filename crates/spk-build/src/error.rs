// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use miette::Diagnostic;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Diagnostic, Debug, Error)]
#[diagnostic(
    url(
        "https://getspk.io/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spk::generic"))
    )
)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(forward(0))]
    Build(#[from] crate::build::BuildError),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    Collection(#[from] crate::build::CollectionError),
    #[error("Failed to create directory {0}")]
    DirectoryCreateError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to open file {0}")]
    FileOpenError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to write file {0}")]
    FileWriteError(std::path::PathBuf, #[source] std::io::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    ProcessSpawnError(spfs::Error),
    #[error("Package validation failed")]
    ValidationFailed {
        #[related]
        errors: Vec<crate::validation::Error>,
    },
    #[error(transparent)]
    #[diagnostic(forward(0))]
    Error(#[from] spfs::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    BuildManifest(#[from] spfs::tracking::manifest::MkError),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkExecError(#[from] spk_exec::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentError(#[from] spk_schema::ident::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSolverError(#[from] spk_solve::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSpecError(#[from] spk_schema::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkStorageError(#[from] spk_storage::Error),
    #[error("Error: {0}")]
    String(String),

    #[error("Use of obsolete validators via 'build.validation.disabled'")]
    #[diagnostic(
        help = "Replace them with the new 'build.validation.rules', as appropriate. http://getspk.io/ref/spec/#validationspec"
    )]
    UseOfObsoleteValidators,
}
