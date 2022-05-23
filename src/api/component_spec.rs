// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::{TryFrom, TryInto};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[cfg(test)]
#[path = "./component_spec_test.rs"]
mod component_spec_test;

/// Defines a named package component.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ComponentSpec {
    pub name: Component,
    #[serde(default)]
    pub files: FileMatcher,
    #[serde(default)]
    pub uses: Vec<Component>,
    #[serde(default)]
    pub requirements: super::RequirementsList,
    #[serde(default)]
    pub embedded: super::EmbeddedPackagesList,
}

impl ComponentSpec {
    /// Create a new, empty component with the given name
    pub fn new<S: TryInto<Component, Error = Error>>(name: S) -> Result<Self> {
        let name = name.try_into()?;
        Ok(Self {
            name,
            uses: Default::default(),
            files: Default::default(),
            requirements: Default::default(),
            embedded: Default::default(),
        })
    }

    /// Generate the default build component
    /// (used when none is provided by the package)
    pub fn default_build() -> Self {
        Self {
            name: Component::Build,
            uses: Default::default(),
            files: FileMatcher::all(),
            requirements: Default::default(),
            embedded: Default::default(),
        }
    }

    /// Generate the default run component
    /// (used when none is provided by the package)
    pub fn default_run() -> Self {
        Self {
            name: Component::Run,
            uses: Default::default(),
            files: FileMatcher::all(),
            requirements: Default::default(),
            embedded: Default::default(),
        }
    }
}

/// Identifies a component by name
#[derive(Debug, Hash, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum Component {
    All,
    Build,
    Run,
    Source,
    Named(String),
}

impl Component {
    /// Parse a component name from a string, ensuring that it's valid
    pub fn parse<S: AsRef<str>>(source: S) -> Result<Self> {
        let source = source.as_ref();
        // for now, components follow the same naming requirements as packages
        let _ = super::PkgName::new(source)?;
        Ok(match source {
            "all" => Self::All,
            "run" => Self::Run,
            "build" => Self::Build,
            "src" => Self::Source,
            _ => Self::Named(source.to_string()),
        })
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::All => "all",
            Self::Run => "run",
            Self::Build => "build",
            Self::Source => "src",
            Self::Named(value) => value,
        }
    }

    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    pub fn is_run(&self) -> bool {
        matches!(self, Self::Run)
    }

    pub fn is_build(&self) -> bool {
        matches!(self, Self::Build)
    }

    pub fn is_source(&self) -> bool {
        matches!(self, Self::Source)
    }

    pub fn is_named(&self) -> bool {
        matches!(self, Self::Named(_))
    }
}

impl std::str::FromStr for Component {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Component {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

impl TryFrom<String> for Component {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        Self::parse(value)
    }
}

impl std::fmt::Display for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl AsRef<str> for Component {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'de> Deserialize<'de> for Component {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Component::try_from(value).map_err(|err| serde::de::Error::custom(err.to_string()))
    }
}

impl Serialize for Component {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

/// Holds a set of valid file patterns for identifying
/// files in the spfs filesystem after a build
#[derive(Clone)]
pub struct FileMatcher {
    rules: Vec<String>,
    gitignore: ignore::gitignore::Gitignore,
}

impl std::fmt::Debug for FileMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileMatcher")
            .field("rules", &self.rules)
            // Skip the `gitignore` field since it is really noisy.
            .field("gitignore", &"<elided>")
            .finish()
    }
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

impl Ord for FileMatcher {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rules.cmp(&other.rules)
    }
}

impl PartialOrd for FileMatcher {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
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
        self.gitignore
            .matched_path_or_any_parents(path, is_dir)
            .is_ignore()
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
