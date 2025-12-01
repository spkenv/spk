// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident::{BuildIdent, PinnableRequest};
use spk_schema_foundation::name::PkgName;

use crate::foundation::option_map::OptionMap;
use crate::v0::EmbeddedInstallSpec;
use crate::{ComponentSpecList, RecipeComponentSpec, RequirementsList, Result};

#[cfg(test)]
#[path = "./embedded_recipe_install_spec_test.rs"]
mod embedded_recipe_install_spec_test;

/// A set of structured installation parameters for a package.
#[derive(
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    Hash,
    is_default_derive_macro::IsDefault,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
)]
#[serde(from = "RawEmbeddedRecipeInstallSpec")]
pub struct EmbeddedRecipeInstallSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList<PinnableRequest>,
    #[serde(default)]
    pub components: ComponentSpecList<RecipeComponentSpec>,
}

impl EmbeddedRecipeInstallSpec {
    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins(
        self,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<&PkgName, &BuildIdent>,
    ) -> Result<EmbeddedInstallSpec> {
        Ok(EmbeddedInstallSpec {
            requirements: self
                .requirements
                .render_all_pins(options, resolved_by_name)?,
            components: self.components.render_all_pins(options, resolved_by_name)?,
        })
    }

    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins_from_iter<'a>(
        self,
        options: &OptionMap,
        resolved: impl Iterator<Item = &'a BuildIdent>,
    ) -> Result<EmbeddedInstallSpec> {
        let resolved_by_name = resolved.map(|x| (x.name(), x)).collect();
        self.render_all_pins(options, &resolved_by_name)
    }
}

impl From<EmbeddedInstallSpec> for EmbeddedRecipeInstallSpec {
    fn from(install: EmbeddedInstallSpec) -> Self {
        Self {
            requirements: install.requirements.into(),
            components: install.components.into(),
        }
    }
}

impl From<RawEmbeddedRecipeInstallSpec> for EmbeddedRecipeInstallSpec {
    fn from(raw: RawEmbeddedRecipeInstallSpec) -> Self {
        Self {
            requirements: raw.requirements,
            components: raw.components,
        }
    }
}

/// A raw, unvalidated install spec.
#[derive(Deserialize)]
struct RawEmbeddedRecipeInstallSpec {
    #[serde(default)]
    requirements: RequirementsList<PinnableRequest>,
    #[serde(default)]
    components: ComponentSpecList<RecipeComponentSpec>,
}
