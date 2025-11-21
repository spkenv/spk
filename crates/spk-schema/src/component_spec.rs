// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::convert::TryInto;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident::PinnedRequest;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::spec_ops::{ComponentFileMatchMode, HasBuildIdent};

use super::RequirementsList;
use crate::component_spec_list::ComponentSpecDefaults;
use crate::foundation::ident_component::Component;
use crate::foundation::spec_ops::{ComponentOps, FileMatcher};
use crate::ident::RequestWithOptions;
use crate::{RecipeComponentSpec, Result};

#[cfg(test)]
#[path = "./component_spec_test.rs"]
mod component_spec_test;

#[derive(Deserialize)]
struct RawComponentSpec {
    name: Component,
    #[serde(default)]
    files: FileMatcher,
    #[serde(default)]
    uses: Vec<Component>,
    #[serde(default)]
    requirements: super::RequirementsList<PinnedRequest>,
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

/// Defines a named package component in a build package.
///
/// See [`crate::v0::RecipeComponentSpec`] for the type used by package recipes.
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
    requirements: RequirementsList<PinnedRequest>,
    #[serde(
        default,
        skip_serializing_if = "super::ComponentEmbeddedPackagesList::is_fabricated"
    )]
    pub embedded: super::ComponentEmbeddedPackagesList,

    #[serde(default)]
    pub file_match_mode: ComponentFileMatchMode,
    #[serde(skip)]
    requirements_with_options: RequirementsList<RequestWithOptions>,
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
    pub fn requirements(&self) -> &RequirementsList<PinnedRequest> {
        &self.requirements
    }

    /// Read-write access to requirements
    pub fn requirements_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut RequirementsList<PinnedRequest>) -> R,
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

    pub(crate) fn new_from_recipe<K, R>(
        spec: RecipeComponentSpec,
        options: &OptionMap,
        resolved_by_name: &HashMap<K, R>,
    ) -> Result<ComponentSpec>
    where
        K: Eq + std::hash::Hash,
        K: std::borrow::Borrow<PkgName>,
        R: HasBuildIdent,
    {
        let RecipeComponentSpec {
            name,
            uses,
            files,
            requirements,
            embedded,
            file_match_mode,
        } = spec;
        let requirements = requirements.render_all_pins(options, resolved_by_name)?;
        Ok(ComponentSpec {
            name,
            uses,
            files,
            requirements_with_options: (options.iter(), &requirements).into(),
            requirements,
            embedded,
            file_match_mode,
        })
    }
}

impl ComponentSpecDefaults for ComponentSpec {
    fn default_build() -> Self {
        Self::default_build()
    }

    fn default_run() -> Self {
        Self::default_run()
    }
}

impl ComponentOps for ComponentSpec {
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
