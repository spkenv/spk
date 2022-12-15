// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema::BuildIdent;
use spk_solve::Request;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Build(#[from] crate::build::BuildError),
    #[error(transparent)]
    Collection(#[from] crate::build::CollectionError),
    #[error("Failed to create directory {0}")]
    DirectoryCreateError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to open file {0}")]
    FileOpenError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to write file {0}")]
    FileWriteError(std::path::PathBuf, #[source] std::io::Error),
    #[error(transparent)]
    ProcessSpawnError(spfs::Error),
    #[error("Package must include a build requirement for {request}, because it's being built against {required_by}. However, {problem}.")]
    MissingDownstreamBuildRequest {
        /// The package that was in the build environment and created the need for this request
        required_by: BuildIdent,
        /// The minimum request that is required downstream
        request: Request,
        /// Additional reasoning why an existing request was not sufficient
        problem: String,
    },
    #[error("Package must include a runtime requirement for {request}, because it's being built against {required_by}. However, {problem}.")]
    MissingDownstreamRuntimeRequest {
        /// The package that was in the build environment and created the need for this request
        required_by: BuildIdent,
        /// The minimum request that is required downstream
        request: Request,
        /// Additional reasoning why an existing request was not sufficient
        problem: String,
    },
    #[error(transparent)]
    SPFS(#[from] spfs::Error),
    #[error(transparent)]
    SpkExecError(#[from] spk_exec::Error),
    #[error(transparent)]
    SpkIdentError(#[from] spk_schema::ident::Error),
    #[error(transparent)]
    SpkSolverError(#[from] spk_solve::Error),
    #[error(transparent)]
    SpkSpecError(#[from] spk_schema::Error),
    #[error(transparent)]
    SpkStorageError(#[from] spk_storage::Error),
    #[error("Error: {0}")]
    String(String),
}
