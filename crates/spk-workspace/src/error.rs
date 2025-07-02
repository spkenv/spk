// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Errors reported by this crate.

use std::path::PathBuf;

/// Errors that can occur when building a workspace from a path on disk.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum FromPathError {
    /// Error loading a workspace file
    #[error(transparent)]
    #[diagnostic(forward(0))]
    LoadWorkspaceFileError(#[from] LoadWorkspaceFileError),
    /// Error building a workspace from a file
    #[error(transparent)]
    #[diagnostic(forward(0))]
    FromFileError(#[from] FromFileError),
}

/// Errors that can occur when building a workspace from a glob pattern.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum FromFileError {
    /// Error parsing the glob pattern
    #[error("invalid glob pattern")]
    PatternError(#[from] glob::PatternError),
    /// Error processing a glob pattern against the filesystem
    #[error("failed to process glob pattern")]
    GlobError(#[from] glob::GlobError),
}

/// Errors that can occur when building a workspace.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum BuildError {
    /// Error loading a package recipe for the workspace
    #[error("failed to load template in workspace: {file:?}")]
    TemplateLoadError {
        /// The file that failed to load
        file: std::path::PathBuf,
        /// The underlying error that occurred
        source: Box<spk_schema::Error>,
    },
    /// A template was found but has no discernible name
    #[error("template cannot be loaded into workspace since it has no name defined: {file:?}")]
    UnnamedTemplate {
        /// The file that could not be loaded
        file: std::path::PathBuf,
    },
}

/// Errors that can occur when loading a workspace file.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum LoadWorkspaceFileError {
    /// The workspace file was not found for a given path
    #[error(
        "workspace not found, no {} in {0:?} or any parent",
        crate::WorkspaceFile::FILE_NAME
    )]
    WorkspaceNotFound(PathBuf),
    /// The workspace file was not found at a given path
    #[error("'{}' not found in {0:?}", crate::WorkspaceFile::FILE_NAME)]
    NoWorkspaceFile(PathBuf),
    /// Error reading the workspace file
    #[error(transparent)]
    ReadFailed(std::io::Error),
    /// Error deserializing the workspace file
    #[error(transparent)]
    InvalidYaml(format_serde_error::SerdeError),
}
