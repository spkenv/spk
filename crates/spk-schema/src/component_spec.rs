// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};

use crate::foundation::ident_component::Component;
use crate::foundation::spec_ops::{ComponentOps, FileMatcher};
use crate::Package;

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
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct ComponentSpec<EmbeddedStub> {
    pub name: Component,
    #[serde(default)]
    pub files: FileMatcher,
    #[serde(default)]
    pub uses: Vec<Component>,
    #[serde(default)]
    pub requirements: super::RequirementsList,
    #[serde(default = "Vec::new")]
    pub embedded: Vec<EmbeddedStub>,
    #[serde(default)]
    pub file_match_mode: ComponentFileMatchMode,
}

impl<P> ComponentSpec<P> {
    /// Create a new, empty component with the given name
    pub fn new(name: Component) -> Self {
        Self {
            name,
            uses: Default::default(),
            files: Default::default(),
            requirements: Default::default(),
            embedded: Default::default(),
            file_match_mode: Default::default(),
        }
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

impl<P> ComponentOps for ComponentSpec<P>
where
    P: Package,
{
    fn files(&self) -> &FileMatcher {
        &self.files
    }
}
