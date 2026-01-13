// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident::PinnableRequest;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::spec_ops::HasBuildIdent;

use crate::foundation::option_map::OptionMap;
use crate::v0::EmbeddedInstallSpec;
use crate::{ComponentSpecList, RecipeComponentSpec, RequirementsList, Result};

#[cfg(test)]
#[path = "./embedded_recipe_install_spec_test.rs"]
mod embedded_recipe_install_spec_test;

/// A set of structured installation parameters for a package.
///
/// This represents the `install` section of an embedded package within a
/// recipe. Once built, [`super::EmbeddedInstallSpec`] is used.
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
    pub fn render_all_pins<K, R>(
        self,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<K, R>,
    ) -> Result<EmbeddedInstallSpec>
    where
        K: Eq + std::hash::Hash,
        K: std::borrow::Borrow<PkgName>,
        R: HasBuildIdent,
    {
        Ok(EmbeddedInstallSpec {
            requirements: self
                .requirements
                .render_all_pins(options, resolved_by_name)?,
            components: self.components.render_all_pins(options, resolved_by_name)?,
        })
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
