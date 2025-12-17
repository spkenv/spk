// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{PinnableRequest, PinnedRequest};
use spk_schema_foundation::name::{OptName, PkgName};
use spk_schema_foundation::spec_ops::{HasBuildIdent, Named};

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
    Deserialize,
    Eq,
    Hash,
    is_default_derive_macro::IsDefault,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
)]
#[serde(
    from = "RawEmbeddedInstallSpec<Request>",
    bound = "Request: DeserializeOwned + Named<OptName>"
)]
pub struct EmbeddedInstallSpec<Request: DeserializeOwned + Named<OptName> + PartialEq + Serialize> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList<Request>,
    #[serde(default)]
    pub components: ComponentSpecList<Request>,
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    pub environment: EnvOpList,
}

// Using derived Default adds a `Request: Default` bound, which we don't want.
impl<Request> Default for EmbeddedInstallSpec<Request>
where
    Request: DeserializeOwned + Named<OptName> + PartialEq + Serialize,
{
    fn default() -> Self {
        Self {
            requirements: RequirementsList::default(),
            components: ComponentSpecList::default(),
            environment: EnvOpList::default(),
        }
    }
}

impl EmbeddedInstallSpec<PinnableRequest> {
    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<K, R>(
        self,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<K, R>,
    ) -> Result<EmbeddedInstallSpec<PinnedRequest>>
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
            environment: self.environment,
        })
    }
}

impl From<EmbeddedInstallSpec<PinnedRequest>> for EmbeddedInstallSpec<PinnableRequest> {
    fn from(install: EmbeddedInstallSpec<PinnedRequest>) -> Self {
        Self {
            requirements: install.requirements.into(),
            components: install.components.into(),
            environment: install.environment,
        }
    }
}

impl<Request> From<RawEmbeddedInstallSpec<Request>> for EmbeddedInstallSpec<Request>
where
    Request: DeserializeOwned + Named<OptName> + PartialEq + Serialize,
{
    fn from(raw: RawEmbeddedInstallSpec<Request>) -> Self {
        Self {
            requirements: raw.requirements,
            components: raw.components,
            environment: raw.environment,
        }
    }
}

/// A raw, unvalidated install spec.
#[derive(Deserialize)]
#[serde(bound = "Request: DeserializeOwned + Named<OptName> + Serialize")]
struct RawEmbeddedInstallSpec<Request> {
    #[serde(default)]
    requirements: RequirementsList<Request>,
    #[serde(default)]
    components: ComponentSpecList<Request>,
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
