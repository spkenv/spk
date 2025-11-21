// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryInto;

use serde::{Deserialize, Serialize};

use crate::foundation::ident_component::Component;
use crate::foundation::spec_ops::{ComponentOps, FileMatcher};
use crate::ident::RequestWithOptions;
use crate::{RequirementsList, Result};

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

#[derive(Deserialize)]
struct RawComponentSpec {
    name: Component,
    #[serde(default)]
    files: FileMatcher,
    #[serde(default)]
    uses: Vec<Component>,
    #[serde(default)]
    requirements: super::RequirementsList,
    #[serde(default)]
    embedded: super::ComponentEmbeddedPackagesList,
    #[serde(default)]
    file_match_mode: ComponentFileMatchMode,
}

impl From<RawComponentSpec> for ComponentSpec {
    fn from(raw: RawComponentSpec) -> Self {
        let mut spec = Self {
            name: raw.name,
            files: raw.files,
            uses: raw.uses,
            requirements: raw.requirements,
            embedded: raw.embedded,
            file_match_mode: raw.file_match_mode,
            requirements_with_options: RequirementsList::<RequestWithOptions>::default(),
        };
        spec.update_requirements_with_options();
        spec
    }
}

/// Defines a named package component.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(from = "RawComponentSpec")]
pub struct ComponentSpec {
    pub name: Component,
    #[serde(default)]
    pub files: FileMatcher,
    #[serde(default)]
    pub uses: Vec<Component>,
    // This field is private to update `requirements_with_options` when it is
    // modified.
    #[serde(default)]
    requirements: super::RequirementsList,
    #[serde(
        default,
        skip_serializing_if = "super::ComponentEmbeddedPackagesList::is_fabricated"
    )]
    pub embedded: super::ComponentEmbeddedPackagesList,

    #[serde(default)]
    pub file_match_mode: ComponentFileMatchMode,
    #[serde(skip)]
    requirements_with_options: super::RequirementsList<RequestWithOptions>,
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
            requirements_with_options: Default::default(),
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
            requirements_with_options: Default::default(),
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
            requirements_with_options: Default::default(),
        }
    }

    /// Generate the default source component
    pub fn default_source() -> Self {
        Self {
            name: Component::Source,
            uses: Default::default(),
            files: FileMatcher::all(),
            requirements: Default::default(),
            embedded: Default::default(),
            file_match_mode: Default::default(),
            requirements_with_options: Default::default(),
        }
    }

    /// Read-only access to requirements
    #[inline]
    pub fn requirements(&self) -> &RequirementsList {
        &self.requirements
    }

    /// Read-write access to requirements
    pub fn requirements_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut RequirementsList) -> R,
    {
        let r = f(&mut self.requirements);
        self.update_requirements_with_options();
        r
    }

    /// Read-only access to requirements_with_options
    #[inline]
    pub fn requirements_with_options(&self) -> &RequirementsList<RequestWithOptions> {
        &self.requirements_with_options
    }

    fn update_requirements_with_options(&mut self) {
        self.requirements_with_options = (&self.requirements).into();
    }
}

impl ComponentOps for ComponentSpec {
    fn files(&self) -> &FileMatcher {
        &self.files
    }
}
