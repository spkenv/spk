use std::path::Path;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::FromYaml;

use crate::error::LoadWorkspaceError;

#[cfg(test)]
#[path = "spec_test.rs"]
mod spec_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct Workspace {
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "glob_from_str")]
    pub recipes: Vec<glob::Pattern>,
}

impl Workspace {
    pub const FILE_NAME: &str = "workspace.spk.yaml";

    /// Load a workspace from its root directory on disk
    pub fn load<P: AsRef<Path>>(root: P) -> Result<Self, LoadWorkspaceError> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|_| LoadWorkspaceError::NoWorkspaceFile(root.as_ref().into()))?;

        let workspace_file = std::fs::read_to_string(root.join(Workspace::FILE_NAME))
            .map_err(LoadWorkspaceError::ReadFailed)?;
        Workspace::from_yaml(workspace_file).map_err(LoadWorkspaceError::InvalidYaml)
    }

    /// Load the workspace for a given dir, looking at parent directories
    /// as necessary to find the workspace root
    pub fn discover<P: AsRef<Path>>(cwd: P) -> Result<Self, LoadWorkspaceError> {
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
        let mut candidate = cwd.clone();
        let mut last_found = None;

        loop {
            if candidate.join(Workspace::FILE_NAME).is_file() {
                last_found = Some(candidate.clone());
            }
            if !candidate.pop() {
                break;
            }
        }
        match last_found {
            Some(path) => Self::load(path),
            None => Err(LoadWorkspaceError::WorkspaceNotFound(cwd)),
        }
    }
}

mod glob_from_str {
    use serde::{Deserializer, Serialize, Serializer};

    pub fn serialize<S>(patterns: &Vec<glob::Pattern>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let patterns: Vec<_> = patterns.iter().map(|p| p.as_str()).collect();
        patterns.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<glob::Pattern>, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// Visits a serialized string, decoding it as a digest
        struct PatternVisitor;

        impl<'de> serde::de::Visitor<'de> for PatternVisitor {
            type Value = Vec<glob::Pattern>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a glob pattern")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut patterns = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(pattern) = seq.next_element()? {
                    let pattern = glob::Pattern::new(pattern).map_err(serde::de::Error::custom)?;
                    patterns.push(pattern);
                }
                Ok(patterns)
            }
        }
        deserializer.deserialize_seq(PatternVisitor)
    }
}
