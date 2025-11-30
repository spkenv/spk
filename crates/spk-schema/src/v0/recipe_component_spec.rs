// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryInto;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident::PinnableRequest;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::spec_ops::{ComponentFileMatchMode, HasBuildIdent};

use crate::component_spec_list::ComponentSpecDefaults;
use crate::foundation::ident_component::Component;
use crate::foundation::spec_ops::{ComponentOps, FileMatcher};
use crate::{ComponentSpec, RequirementsList, Result};

#[cfg(test)]
#[path = "./recipe_component_spec_test.rs"]
mod recipe_component_spec_test;

/// Defines a named package component in a recipe.
///
/// Once built, [`crate::ComponentSpec`] is used.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RecipeComponentSpec {
    pub name: Component,
    #[serde(default)]
    pub files: FileMatcher,
    #[serde(default)]
    pub uses: Vec<Component>,
    #[serde(default)]
    pub requirements: RequirementsList<PinnableRequest>,
    #[serde(
        default,
        skip_serializing_if = "crate::ComponentEmbeddedPackagesList::is_fabricated"
    )]
    pub embedded: crate::ComponentEmbeddedPackagesList,

    #[serde(default)]
    pub file_match_mode: ComponentFileMatchMode,
}

impl RecipeComponentSpec {
    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<K, R>(
        self,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<K, R>,
    ) -> Result<ComponentSpec>
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

impl RecipeComponentSpec {
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

impl ComponentSpecDefaults for RecipeComponentSpec {
    fn default_build() -> Self {
        Self::default_build()
    }

    fn default_run() -> Self {
        Self::default_run()
    }
}

impl ComponentOps for RecipeComponentSpec {
    #[inline]
    fn file_match_mode(&self) -> &ComponentFileMatchMode {
        &self.file_match_mode
    }
    #[inline]
    fn files(&self) -> &FileMatcher {
        &self.files
    }
    #[inline]
    fn name(&self) -> &Component {
        &self.name
    }
    #[inline]
    fn uses(&self) -> &[Component] {
        &self.uses
    }
}

impl From<ComponentSpec> for RecipeComponentSpec {
    fn from(other: ComponentSpec) -> Self {
        Self {
            name: other.name,
            files: other.files,
            uses: other.uses,
            requirements: other.requirements.into(),
            embedded: other.embedded,
            file_match_mode: other.file_match_mode,
        }
    }
}
