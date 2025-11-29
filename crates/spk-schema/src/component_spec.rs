// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryInto;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident::{PinnableRequest, PinnedRequest};
use spk_schema_foundation::name::{OptName, PkgName};
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::spec_ops::HasBuildIdent;

use super::RequirementsList;
use crate::Result;
use crate::foundation::ident_component::Component;
use crate::foundation::spec_ops::{ComponentOps, FileMatcher, Named};

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
#[serde(bound = "Request: DeserializeOwned + Named<OptName> + Serialize")]
pub struct ComponentSpec<Request> {
    pub name: Component,
    #[serde(default)]
    pub files: FileMatcher,
    #[serde(default)]
    pub uses: Vec<Component>,
    #[serde(default)]
    pub requirements: RequirementsList<Request>,
    #[serde(
        default,
        skip_serializing_if = "super::ComponentEmbeddedPackagesList::is_fabricated"
    )]
    pub embedded: super::ComponentEmbeddedPackagesList,

    #[serde(default)]
    pub file_match_mode: ComponentFileMatchMode,
}

impl ComponentSpec<PinnableRequest> {
    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<K, R>(
        self,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<K, R>,
    ) -> Result<ComponentSpec<PinnedRequest>>
    where
        K: Eq + std::hash::Hash,
        K: std::borrow::Borrow<PkgName>,
        R: HasBuildIdent,
    {
        Ok(ComponentSpec {
            name: self.name,
            files: self.files,
            uses: self.uses,
            requirements: self
                .requirements
                .render_all_pins(options, resolved_by_name)?,
            embedded: self.embedded,
            file_match_mode: self.file_match_mode,
        })
    }
}

impl<Request> ComponentSpec<Request> {
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

impl<Request> ComponentOps for ComponentSpec<Request> {
    fn files(&self) -> &FileMatcher {
        &self.files
    }
}

impl From<ComponentSpec<PinnedRequest>> for ComponentSpec<PinnableRequest> {
    fn from(spec: ComponentSpec<PinnedRequest>) -> Self {
        Self {
            name: spec.name,
            files: spec.files,
            uses: spec.uses,
            requirements: spec.requirements.into(),
            embedded: spec.embedded,
            file_match_mode: spec.file_match_mode,
        }
    }
}
