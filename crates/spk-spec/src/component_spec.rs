// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryInto;

use serde::{Deserialize, Serialize};
use spk_ident_component::Component;
use spk_spec_ops::{ComponentOps, FileMatcher};

use crate::Result;

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
    pub fn new<S: TryInto<Component, Error = spk_ident_component::Error>>(name: S) -> Result<Self> {
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

impl ComponentOps for ComponentSpec {
    fn files(&self) -> &FileMatcher {
        &self.files
    }
}
