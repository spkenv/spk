// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryInto;

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::foundation::ident_component::Component;
use crate::foundation::spec_ops::{ComponentOps, FileMatcher};

#[cfg(test)]
#[path = "./component_spec_test.rs"]
mod component_spec_test;

/// Control how files are filtered between components.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ComponentFileMatchMode {
    /// Matching files are always included.
    #[default]
    All,
    /// Matching files are only included if they haven't already been matched
    /// by a previously defined component.
    Remaining,
}

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
    #[serde(
        default,
        skip_serializing_if = "super::ComponentEmbeddedPackagesList::is_fabricated"
    )]
    pub embedded: super::ComponentEmbeddedPackagesList,

    #[serde(default)]
    pub file_match_mode: ComponentFileMatchMode,
}

impl ComponentSpec {
    /// Create a new, empty component with the given name
    pub fn new<S: TryInto<Component, Error = crate::foundation::ident_component::Error>>(
        name: S,
    ) -> Result<Self> {
        let name = name.try_into()?;
        Ok(Self {
            name,
            uses: Default::default(),
            files: Default::default(),
            requirements: Default::default(),
            embedded: Default::default(),
            file_match_mode: Default::default(),
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
            file_match_mode: Default::default(),
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
            file_match_mode: Default::default(),
        }
    }
}

impl ComponentOps for ComponentSpec {
    fn files(&self) -> &FileMatcher {
        &self.files
    }
}
