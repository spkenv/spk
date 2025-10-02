// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Workspace file definition.
//!
//! The format and loading process for workspace yaml files.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use spk_schema::foundation::FromYaml;

use crate::error::LoadWorkspaceFileError;

#[cfg(test)]
#[path = "file_test.rs"]
mod file_test;

/// Describes a workspace configuration.
///
/// Contains information about the layout of the workspace,
/// and where to find data, usually loaded from a file on disk.
/// It must still be fully validated and loaded into a
/// [`super::Workspace`] to be operated on.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize)]
pub struct WorkspaceFile {
    /// The package recipes that are part of this workspace
    #[serde(default)]
    pub recipes: Vec<RecipesItem>,
}

impl WorkspaceFile {
    /// The expected file name for a workspace file
    pub const FILE_NAME: &str = "workspace.spk.yaml";

    /// Load a workspace from its root directory on disk
    pub fn load<P: AsRef<Path>>(root: P) -> Result<Self, LoadWorkspaceFileError> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|_| LoadWorkspaceFileError::NoWorkspaceFile(root.as_ref().into()))?;

        let workspace_file = std::fs::read_to_string(root.join(WorkspaceFile::FILE_NAME))
            .map_err(LoadWorkspaceFileError::ReadFailed)?;
        WorkspaceFile::from_yaml(workspace_file).map_err(LoadWorkspaceFileError::InvalidYaml)
    }

    /// Load the workspace for a given dir, looking at parent directories
    /// as necessary to find the workspace root. Returns the workspace root directory
    /// that was found, if any.
    pub fn discover<P: AsRef<Path>>(cwd: P) -> Result<(Self, PathBuf), LoadWorkspaceFileError> {
        let cwd = if cwd.as_ref().is_absolute() {
            cwd.as_ref().to_owned()
        } else {
            // prefer PWD if available, since it may be more representative of
            // how the user arrived at the current dir and avoids dereferencing
            // symlinks that could otherwise make error messages harder to understand
            match std::env::var("PWD").ok() {
                Some(pwd) => Path::new(&pwd).join(cwd),
                None => std::env::current_dir().unwrap_or_default().join(cwd),
            }
        };

        let mut candidate: std::path::PathBuf = cwd.clone();
        loop {
            if candidate.join(WorkspaceFile::FILE_NAME).is_file() {
                return Self::load(&candidate).map(|l| (l, candidate));
            }
            if !candidate.pop() {
                break;
            }
        }
        Err(LoadWorkspaceFileError::WorkspaceNotFound(cwd))
    }
}
/// One item in the list of recipes for a workspace.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct RecipesItem {
    /// The path to a recipe file or files in the workspace.
    pub path: glob::Pattern,
}

impl<'de> serde::de::Deserialize<'de> for RecipesItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct RecipeCollectorVisitor;

        impl<'de> serde::de::Visitor<'de> for RecipeCollectorVisitor {
            type Value = RecipesItem;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a glob pattern")
            }

            fn visit_str<E>(self, v: &str) -> Result<RecipesItem, E>
            where
                E: serde::de::Error,
            {
                let path = glob::Pattern::new(v).map_err(serde::de::Error::custom)?;
                Ok(RecipesItem { path })
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                #[derive(Deserialize)]
                struct RawRecipeItem {
                    path: String,
                }

                let RawRecipeItem { path } =
                    RawRecipeItem::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                self.visit_str(&path)
            }
        }

        deserializer.deserialize_any(RecipeCollectorVisitor)
    }
}
