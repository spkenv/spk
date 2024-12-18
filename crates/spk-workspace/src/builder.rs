// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Find and/or build workspaces.

use std::collections::HashMap;

use crate::error;

/// Used to construct a [`super::Workspace`] either from
/// yaml files on disk or programmatically.
#[derive(Default)]
pub struct WorkspaceBuilder {
    spec_files: HashMap<std::path::PathBuf, crate::file::TemplateConfig>,
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
        self,
        file: crate::file::WorkspaceFile,
    ) -> Result<Self, error::FromFileError> {
        file.recipes.iter().try_fold(self, |builder, pattern| {
            builder.with_glob_pattern(pattern.path.as_str())
        })
    }

    /// Add all recipe files matching a glob pattern to the workspace.
    ///
    /// If the provided pattern is relative, it will be relative to the
    /// current working directory.
    pub fn with_recipes_item(
        mut self,
        item: &crate::file::RecipesItem,
    ) -> Result<Self, error::FromFileError> {
        let mut glob_results = glob::glob(item.path.as_str())?;
        while let Some(path) = glob_results.next().transpose()? {
            self.spec_files
                .entry(path)
                .or_default()
                .update(&item.config);
        }

        Ok(self)
    }

    /// Add all recipe files matching a glob pattern to the workspace.
    ///
    /// If the provided pattern is relative, it will be relative to the
    /// workspace root (if it has one) or current working directory.
    /// All configuration for this path will be left as the defaults
    /// unless already set.
    pub fn with_glob_pattern<S: AsRef<str>>(
        mut self,
        pattern: S,
    ) -> Result<Self, error::FromFileError> {
        let mut glob_results = glob::glob(pattern.as_ref())?;
        while let Some(path) = glob_results.next().transpose()? {
            self.spec_files.entry(path).or_default();
        }

        Ok(self)
    }

    /// Build the workspace as configured.
    pub fn build(self) -> Result<super::Workspace, error::BuildError> {
        let mut workspace = super::Workspace::default();
        for (file, config) in self.spec_files {
            workspace.load_template_file_with_config(file, config)?;
        }
        Ok(workspace)
    }
}
