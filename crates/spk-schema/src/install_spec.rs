// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;
use spk_schema_ident::BuildIdent;

use super::{ComponentSpecList, EmbeddedPackagesList, EnvOp, OpKind, RequirementsList};
use crate::foundation::option_map::OptionMap;
use crate::{InstallSpecKey, LintMessage, LintedItem, Lints, Result};

#[cfg(test)]
#[path = "./install_spec_test.rs"]
mod install_spec_test;

/// A set of structured installation parameters for a package.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct InstallSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded: EmbeddedPackagesList,
    #[serde(default)]
    pub components: ComponentSpecList,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment: Vec<EnvOp>,
}

impl<D> Lints for InstallSpecVisitor<D>
where
    D: Default,
{
    fn lints(&mut self) -> Vec<LintMessage> {
        std::mem::take(&mut self.lints)
    }
}

#[derive(Default)]
struct InstallSpecVisitor<D>
where
    D: Default,
{
    requirements: RequirementsList,
    embedded: EmbeddedPackagesList,
    components: ComponentSpecList,
    environment: Vec<EnvOp>,
    lints: Vec<LintMessage>,
    _phantom: PhantomData<D>,
}

impl<D> From<InstallSpecVisitor<D>> for InstallSpec
where
    D: Default,
{
    fn from(value: InstallSpecVisitor<D>) -> Self {
        Self {
            requirements: value.requirements,
            embedded: value.embedded,
            components: value.components,
            environment: value.environment,
        }
    }
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

impl<'de> Deserialize<'de> for InstallSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_map(InstallSpecVisitor::<InstallSpec>::default())
    }
}

impl<'de> Deserialize<'de> for LintedItem<InstallSpec> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_map(InstallSpecVisitor::<LintedItem<InstallSpec>>::default())
    }
}

impl<'de, D> serde::de::Visitor<'de> for InstallSpecVisitor<D>
where
    D: Default + From<InstallSpecVisitor<D>>,
{
    type Value = D;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a package specification")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "requirements" => self.requirements = map.next_value::<RequirementsList>()?,
                "embedded" => self.embedded = map.next_value::<EmbeddedPackagesList>()?,
                "components" => self.components = map.next_value::<ComponentSpecList>()?,
                "environment" => self.environment = map.next_value::<Vec<EnvOp>>()?,
                unknown_config => {
                    self.lints
                        .push(LintMessage::UnknownInstallSpecKey(InstallSpecKey::new(
                            unknown_config,
                        )));
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        if self
            .environment
            .iter()
            .filter(|&e| e.kind() == OpKind::Priority)
            .count()
            > 1
        {
            return Err(serde::de::Error::custom(
                "Multiple priority configs cannot be set",
            ));
        }

        Ok(self.into())
    }
}
