// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident::PinnedRequest;

use crate::{ComponentSpec, ComponentSpecList, RequirementsList};

#[cfg(test)]
#[path = "./embedded_install_spec_test.rs"]
mod embedded_install_spec_test;

/// A set of structured installation parameters for a package.
///
/// This represents the `install` section of an embedded package within a built
/// package. See [`super::EmbeddedRecipeInstallSpec`] for the type used by
/// recipes.
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
#[serde(from = "RawEmbeddedInstallSpec")]
pub struct EmbeddedInstallSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList<PinnedRequest>,
    #[serde(default)]
    pub components: ComponentSpecList<ComponentSpec>,
}

impl From<RawEmbeddedInstallSpec> for EmbeddedInstallSpec {
    fn from(raw: RawEmbeddedInstallSpec) -> Self {
        Self {
            requirements: raw.requirements,
            components: raw.components,
        }
    }
}

/// A raw, unvalidated install spec.
#[derive(Deserialize)]
struct RawEmbeddedInstallSpec {
    #[serde(default)]
    requirements: RequirementsList<PinnedRequest>,
    #[serde(default)]
    components: ComponentSpecList<ComponentSpec>,
}
