// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[cfg(test)]
#[path = "./component_spec_test.rs"]
mod component_spec_test;

/// Defines a named package component.
#[pyclass]
#[derive(Debug, Hash, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComponentSpec {
    #[pyo3(get)]
    #[serde(deserialize_with = "deserialize_component_name")]
    name: String,
    #[serde(default)]
    pub files: FileMatcher,
    #[pyo3(get, set)]
    #[serde(default)]
    pub uses: Vec<String>,
    #[pyo3(get, set)]
    #[serde(default)]
    pub compat: Option<super::Compat>,
    #[pyo3(get, set)]
    #[serde(default)]
    pub requirements: super::RequirementsList,
    #[pyo3(get, set)]
    #[serde(default)]
    pub embedded: super::EmbeddedPackagesList,
}

impl ComponentSpec {
    /// Create a new, empty component with the given name
    pub fn new<S: Into<String>>(name: S) -> Result<Self> {
        let name = name.into();
        super::validate_name(&name)?;
        Ok(Self {
            name,
            compat: None,
            uses: Default::default(),
            files: Default::default(),
            requirements: Default::default(),
            embedded: Default::default(),
        })
    }

    /// Generate the default build component
    /// (used when non is provided by the package)
    pub fn default_build() -> Self {
        Self {
            name: "build".to_string(),
            compat: None,
            uses: Default::default(),
            files: FileMatcher::all(),
            requirements: Default::default(),
            embedded: Default::default(),
        }
    }

    /// Generate the default run component
    /// (used when non is provided by the package)
    pub fn default_run() -> Self {
        Self {
            name: "run".to_string(),
            compat: None,
            uses: Default::default(),
            files: FileMatcher::all(),
            requirements: Default::default(),
            embedded: Default::default(),
        }
    }

    /// Generate the default run component.
    /// All of the other component names must be provided.
    /// (used when non is provided by the package)
    pub fn default_all<U, I>(names: U) -> Self
    where
        U: IntoIterator<Item = I>,
        I: Into<String>,
    {
        Self {
            name: "all".to_string(),
            compat: None,
            uses: names.into_iter().map(Into::into).collect(),
            files: Default::default(),
            requirements: Default::default(),
            embedded: Default::default(),
        }
    }

    /// The name of this component.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Holds a set of valid file patterns for identifying
/// files in the spfs filesystem after a build
#[derive(Debug, Clone)]
pub struct FileMatcher {
    rules: Vec<String>,
    gitignore: ignore::gitignore::Gitignore,
}

impl Default for FileMatcher {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            gitignore: ignore::gitignore::Gitignore::empty(),
        }
    }
}

impl std::hash::Hash for FileMatcher {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.rules.hash(state);
    }
}

impl std::cmp::PartialEq for FileMatcher {
    fn eq(&self, other: &Self) -> bool {
        self.rules == other.rules
    }
}

impl std::cmp::Eq for FileMatcher {}

impl FileMatcher {
    /// Construct a matcher from the given match patterns
    ///
    /// These patterns are expected to be gitignore format
    /// where the root of the matching is considered to be /spfs.
    /// This means that to match /spfs/file.txt non-recursively
    /// you would use the pattern /file.txt
    pub fn new<P, I>(rules: P) -> Result<Self>
    where
        P: IntoIterator<Item = I>,
        I: Into<String>,
    {
        let rules: Vec<_> = rules.into_iter().map(Into::into).collect();
        let mut builder = ignore::gitignore::GitignoreBuilder::new("/");
        for rule in rules.iter() {
            builder.add_line(None, rule).map_err(|err| {
                Error::String(format!("Invalid file pattern '{}': {:?}", rule, err))
            })?;
        }
        let gitignore = builder
            .build()
            .map_err(|err| Error::String(format!("Failed to compile file patterns: {:?}", err)))?;
        Ok(Self { rules, gitignore })
    }

    /// Create a new matcher that matches all files
    pub fn all() -> Self {
        // we trust that the '*' rule will always be valid
        Self::new(vec!["*".to_string()]).unwrap()
    }

    /// The list of file match patterns for this component
    pub fn rules(&self) -> &Vec<String> {
        &self.rules
    }

    /// Reports true if the given path matches a rule in this set
    pub fn matches<P: AsRef<std::path::Path>>(&self, path: P, is_dir: bool) -> bool {
        self.gitignore.matched(path, is_dir).is_ignore()
    }
}

impl<'de> Deserialize<'de> for FileMatcher {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let files = Vec::<String>::deserialize(deserializer)?;
        FileMatcher::new(files.into_iter()).map_err(|err| serde::de::Error::custom(err.to_string()))
    }
}

impl Serialize for FileMatcher {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.rules.serialize(serializer)
    }
}

fn deserialize_component_name<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    super::validate_name(&value).map_err(|err| {
        serde::de::Error::invalid_value(
            serde::de::Unexpected::Str(&value),
            &err.to_string().as_str(),
        )
    })?;
    Ok(value)
}
