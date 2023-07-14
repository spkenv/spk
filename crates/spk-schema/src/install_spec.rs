// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use serde::{Deserialize, Serialize};
use spk_schema_ident::BuildIdent;

use super::{ComponentSpecList, EmbeddedPackagesList, EnvConfig, OpKind, RequirementsList};
use crate::foundation::option_map::OptionMap;
use crate::Result;

#[cfg(test)]
#[path = "./install_spec_test.rs"]
mod install_spec_test;

/// A set of structured installation parameters for a package.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct InstallSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded: EmbeddedPackagesList,
    #[serde(default)]
    pub components: ComponentSpecList,
    #[serde(
        default,
        deserialize_with = "deserialize_env_conf",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub environment: Vec<EnvConfig>,
}

impl InstallSpec {
    pub fn is_default(&self) -> bool {
        self.requirements.is_empty() && self.embedded.is_empty() && self.components.is_default()
    }

    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<'a>(
        &mut self,
        options: &OptionMap,
        resolved: impl Iterator<Item = &'a BuildIdent>,
    ) -> Result<()> {
        self.requirements.render_all_pins(options, resolved)
    }
}

fn deserialize_env_conf<'de, D>(deserializer: D) -> std::result::Result<Vec<EnvConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct EnvConfVisitor;

    impl<'de> serde::de::Visitor<'de> for EnvConfVisitor {
        type Value = Vec<EnvConfig>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an environment configuration")
        }

        fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();

            while let Some(elem) = seq.next_element::<EnvConfig>()? {
                if vec.iter().any(|x: &EnvConfig| x.kind() == OpKind::Priority)
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
