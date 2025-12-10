// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::BuildIdent;

use crate::foundation::option_map::OptionMap;
use crate::{ComponentSpecList, EnvOp, EnvOpList, OpKind, RequirementsList, Result};

#[cfg(test)]
#[path = "./embedded_install_spec_test.rs"]
mod embedded_install_spec_test;

/// A set of structured installation parameters for a package.
///
/// This represents the `install` section of an embedded package.
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
    pub requirements: RequirementsList,
    #[serde(default)]
    pub components: ComponentSpecList,
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    pub environment: EnvOpList,
}

impl EmbeddedInstallSpec {
    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<'a>(
        &mut self,
        options: &OptionMap,
        resolved: impl Iterator<Item = &'a BuildIdent>,
    ) -> Result<()> {
        let resolved_by_name = resolved.map(|x| (x.name(), x)).collect();
        self.requirements
            .render_all_pins(options, &resolved_by_name)?;
        for component in self.components.iter_mut() {
            component
                .requirements
                .render_all_pins(options, &resolved_by_name)?;
        }
        Ok(())
    }
}

impl From<RawEmbeddedInstallSpec> for EmbeddedInstallSpec {
    fn from(raw: RawEmbeddedInstallSpec) -> Self {
        Self {
            requirements: raw.requirements,
            components: raw.components,
            environment: raw.environment,
        }
    }
}

/// A raw, unvalidated install spec.
#[derive(Deserialize)]
struct RawEmbeddedInstallSpec {
    #[serde(default)]
    requirements: RequirementsList,
    #[serde(default)]
    components: ComponentSpecList,
    #[serde(default, deserialize_with = "deserialize_env_conf")]
    environment: EnvOpList,
}

fn deserialize_env_conf<'de, D>(deserializer: D) -> std::result::Result<EnvOpList, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct EnvConfVisitor;

    impl<'de> serde::de::Visitor<'de> for EnvConfVisitor {
        type Value = EnvOpList;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an environment configuration")
        }

        fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut vec = EnvOpList::default();

            while let Some(elem) = seq.next_element::<EnvOp>()? {
                if vec.iter().any(|x: &EnvOp| x.kind() == OpKind::Priority)
                    && elem.kind() == OpKind::Priority
                {
                    return Err(serde::de::Error::custom(
                        "Multiple priority config cannot be set.",
                    ));
                };
                vec.push(elem);
            }
            Ok(vec)
        }
    }
    deserializer.deserialize_seq(EnvConfVisitor)
}
