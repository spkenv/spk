// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum FromPathError {
    #[error(transparent)]
    #[diagnostic(forward(0))]
    LoadWorkspaceFileError(#[from] LoadWorkspaceFileError),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    FromFileError(#[from] FromFileError),
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum FromFileError {
    #[error("Invalid glob pattern")]
    PatternError(#[from] glob::PatternError),
    #[error("Failed to process glob pattern")]
    GlobError(#[from] glob::GlobError),
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum BuildError {
    #[error("Failed to load spec from workspace: {file:?}")]
    TemplateLoadError {
        file: std::path::PathBuf,
        source: spk_schema::Error,
    },
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum LoadWorkspaceFileError {
    #[error(
        "workspace not found, no {} in {0:?} or any parent",
        crate::WorkspaceFile::FILE_NAME
    )]
    WorkspaceNotFound(PathBuf),
    #[error("'{}' not found in {0:?}", crate::WorkspaceFile::FILE_NAME)]
    NoWorkspaceFile(PathBuf),
    #[error(transparent)]
    ReadFailed(std::io::Error),
    #[error(transparent)]
    InvalidYaml(format_serde_error::SerdeError),
}
