// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use bracoxide::tokenizer::TokenizationError;
use bracoxide::OxidizationError;
use serde::Deserialize;
use spk_schema::foundation::FromYaml;
use spk_schema::version::Version;

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

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct RecipesItem {
    pub path: glob::Pattern,
    pub config: TemplateConfig,
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
                Ok(RecipesItem {
                    path,
                    config: Default::default(),
                })
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                #[derive(Deserialize)]
                struct RawRecipeItem {
                    path: String,
                    #[serde(flatten)]
                    config: TemplateConfig,
                }

                let RawRecipeItem { path, config } =
                    RawRecipeItem::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                let mut base = self.visit_str(&path)?;
                base.config = config;
                Ok(base)
            }
        }

        deserializer.deserialize_any(RecipeCollectorVisitor)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Default)]
pub struct TemplateConfig {
    /// Ordered set of versions that this template can produce.
    ///
    /// An empty set of versions does not mean that no versions can
    /// be produced, but rather that any can be attempted. It's also
    /// typical for a template to have a single hard-coded version inside
    /// and so not need to specify values for this field.
    pub versions: BTreeSet<Version>,
}

impl TemplateConfig {
    /// Update this config with newly specified data.
    ///
    /// Default values in the provided `other` value do not
    /// overwrite existing data in this instance.
    pub fn update(&mut self, other: Self) {
        let Self { versions } = other;
        if !versions.is_empty() {
            self.versions = versions;
        }
    }
}

impl<'de> serde::de::Deserialize<'de> for TemplateConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct TemplateConfigVisitor;

        impl<'de> serde::de::Visitor<'de> for TemplateConfigVisitor {
            type Value = TemplateConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("additional recipe collection configuration")
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                #[derive(Deserialize)]
                struct RawConfig {
                    versions: Vec<String>,
                }

                let raw_config =
                    RawConfig::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                let mut base = TemplateConfig::default();
                for (i, version_expr) in raw_config.versions.into_iter().enumerate() {
                    let expand_result = bracoxide::bracoxidize(&version_expr);
                    let expanded = match expand_result {
                        Ok(expanded) => expanded,
                        Err(OxidizationError::TokenizationError(TokenizationError::NoBraces))
                        | Err(OxidizationError::TokenizationError(
                            TokenizationError::EmptyContent,
                        ))
                        | Err(OxidizationError::TokenizationError(
                            TokenizationError::FormatNotSupported,
                        )) => {
                            vec![version_expr]
                        }
                        Err(err) => {
                            return Err(serde::de::Error::custom(format!(
                                "invalid brace expansion in position {i}: {err:?}"
                            )))
                        }
                    };
                    for version in expanded {
                        let parsed = Version::from_str(&version).map_err(|err| {
                            serde::de::Error::custom(format!(
                                "brace expansion in position {i} produced invalid version '{version}': {err}"
                            ))
                        })?;
                        base.versions.insert(parsed);
                    }
                }
                Ok(base)
            }
        }

        deserializer.deserialize_map(TemplateConfigVisitor)
    }
}
