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
    root: Option<std::path::PathBuf>,
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
        let (file, root) = crate::file::WorkspaceFile::discover(dir)?;
        self.with_root(root)
            .load_from_file(file)
            .map_err(error::FromPathError::from)
    }

    /// Load all data from a workspace specification.
    pub fn load_from_file(
        self,
        file: crate::file::WorkspaceFile,
    ) -> Result<Self, error::FromFileError> {
        file.recipes
            .iter()
            .try_fold(self, |builder, item| builder.with_recipes_item(item))
    }

    /// Specify the root directory for the workspace.
    ///
    /// This is the path that will be used to resolve all
    /// relative paths and relative glob patterns. If not
    /// specified, the current working directory will be used.
    pub fn with_root(mut self, root: impl Into<std::path::PathBuf>) -> Self {
        self.root = Some(root.into());
        self
    }

    /// Add all recipe files matching a glob pattern to the workspace.
    ///
    /// If the provided pattern is relative, it will be relative to the
    /// current working directory.
    pub fn with_recipes_item(
        mut self,
        item: &crate::file::RecipesItem,
    ) -> Result<Self, error::FromFileError> {
        let with_root = self.root.as_deref().map(|p| p.join(item.path.as_str()));
        let pattern = with_root
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or(item.path.as_str());
        let mut glob_results = glob::glob(pattern)?;
        while let Some(path) = glob_results.next().transpose()? {
            self.spec_files
                .entry(path)
                .or_default()
                .update(item.config.clone());
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
        self,
        pattern: S,
    ) -> Result<Self, error::FromFileError> {
        self.with_recipes_item(&crate::file::RecipesItem {
            path: glob::Pattern::new(pattern.as_ref())?,
            config: Default::default(),
        })
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
