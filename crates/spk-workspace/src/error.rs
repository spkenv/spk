use std::path::PathBuf;

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
