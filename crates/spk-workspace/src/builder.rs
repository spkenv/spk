// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Find and/or build workspaces.

use std::collections::BTreeSet;
use std::error::Error;

use crate::error;

/// Used to construct a [`super::Workspace`] either from
/// yaml files on disk or programmatically.
#[derive(Default)]
pub struct WorkspaceBuilder {
    root: Option<std::path::PathBuf>,
    spec_files: BTreeSet<std::path::PathBuf>,
    ignore_invalid_files: bool,
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
            self.spec_files.insert(path);
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
        })
    }

    /// When true, the workspace build will fail if any recipe files are invalid.
    ///
    /// This defaults to `false` because workspaces are used even in cases
    /// where there is no explicit workspace file found. In these cases it
    /// is seen as unexpected to fail on invalid files, but it can be enabled
    /// in workspace-specific cases where the validation is appropriate.
    pub fn with_ignore_invalid_files(mut self, ignore: bool) -> Self {
        self.ignore_invalid_files = ignore;
        self
    }

    /// Build the workspace as configured.
    pub fn build(self) -> Result<super::Workspace, error::BuildError> {
        let mut workspace = super::Workspace::default();
        for file in self.spec_files {
            match workspace.load_template_file(&file) {
                Ok(_) => {}
                Err(e) if self.ignore_invalid_files => {
                    tracing::warn!(
                        file = file.to_string_lossy().to_string(),
                        err = %e,
                        cause = ?e.source().map(ToString::to_string),
                        "ignoring template file"
                    );
                }
                Err(e) => return Err(e),
            }
        }
        Ok(workspace)
    }
}
