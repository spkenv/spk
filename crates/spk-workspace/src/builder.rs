// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;

use itertools::Itertools;
use spk_schema::TemplateExt;

mod error {
    pub use crate::error::LoadWorkspaceFileError;

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
}

#[derive(Default)]
pub struct WorkspaceBuilder {
    spec_files: Vec<std::path::PathBuf>,
}

impl WorkspaceBuilder {
    /// Load all data from a workspace file discovered using the current directory.
    pub fn load_from_current_dir(self) -> Result<Self, error::FromPathError> {
        self.load_from_dir(".")
    }

    /// Load all data from a workspace file in the given directory.
    pub fn load_from_dir(
        self,
        dir: impl AsRef<std::path::Path>,
    ) -> Result<Self, error::FromPathError> {
        let file = crate::file::WorkspaceFile::discover(dir)?;
        self.load_from_file(file)
            .map_err(error::FromPathError::from)
    }

    /// Load all data from a workspace specification.
    pub fn load_from_file(
        mut self,
        file: crate::file::WorkspaceFile,
    ) -> Result<Self, error::FromFileError> {
        let mut glob_results = file
            .recipes
            .iter()
            .map(|pattern| glob::glob(pattern.path.as_str()))
            .flatten_ok()
            .flatten_ok();
        while let Some(path) = glob_results.next().transpose()? {
            self = self.with_recipe_file(path);
        }

        Ok(self)
    }

    /// Add a recipe file to the workspace.
    pub fn with_recipe_file(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.spec_files.push(path.into());
        self
    }

    pub fn build(self) -> Result<super::Workspace, error::BuildError> {
        let mut templates = HashMap::<_, Vec<_>>::new();
        for file in self.spec_files {
            let template = spk_schema::SpecTemplate::from_file(&file)
                .map_err(|source| error::BuildError::TemplateLoadError { file, source })?;
            templates
                .entry(template.name().cloned())
                .or_default()
                .push(template);
        }
        Ok(crate::Workspace { templates })
    }
}
