// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};

use super::{Error, Result};

#[cfg(test)]
#[path = "./file_matcher_test.rs"]
mod file_matcher_test;

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
