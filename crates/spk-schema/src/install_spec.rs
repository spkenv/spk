// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk
use std::marker::PhantomData;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::spec_ops::Named;
use spk_schema_foundation::IsDefault;
use spk_schema_ident::{BuildIdent, OptVersionIdent};
use spk_schema_foundation::option_map::Stringified;

use crate::component_embedded_packages::ComponentEmbeddedPackage;
use crate::foundation::option_map::OptionMap;
use crate::{
    ComponentSpecList,
    EmbeddedPackagesList,
    EnvOp,
    EnvOpList,
    InstallSpecKey,
    LintMessage,
    LintedItem,
    Lints,
    OpKind,
    Package,
    RequirementsList,
    Result,
};

#[cfg(test)]
#[path = "./install_spec_test.rs"]
mod install_spec_test;

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
#[serde(from = "RawInstallSpec")]
pub struct InstallSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded: EmbeddedPackagesList,
    #[serde(default)]
    pub components: ComponentSpecList,
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    pub environment: EnvOpList,
}

impl<D> Lints for RawInstallSpecVisitor<D>
where
    D: Default,
{
    fn lints(&mut self) -> Vec<LintMessage> {
        for env in self.environment.iter_mut() {
            self.lints.extend(std::mem::take(&mut env.lints));
        }
        std::mem::take(&mut self.lints)
    }
}

#[derive(Default)]
struct RawInstallSpecVisitor<D>
where
    D: Default,
{
    requirements: RequirementsList,
    embedded: EmbeddedPackagesList,
    components: ComponentSpecList,
    environment: LintedItem<EnvOpList>,
    lints: Vec<LintMessage>,
    _phantom: PhantomData<D>,
}

impl<D> From<RawInstallSpecVisitor<D>> for InstallSpec
where
    D: Default,
{
    fn from(value: RawInstallSpecVisitor<D>) -> Self {
        Self {
            requirements: value.requirements,
            embedded: value.embedded,
            components: value.components,
            environment: value
                .environment
                .iter()
                .map(|l| l.item.clone())
                .collect_vec(),
        }
    }
}

impl InstallSpec {
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

impl From<RawInstallSpec> for InstallSpec {
    fn from(raw: RawInstallSpec) -> Self {
        let mut install = Self {
            requirements: raw.requirements,
            embedded: raw.embedded,
            components: raw.components,
            environment: raw.environment,
        };

        if install.embedded.is_empty() {
            // If there are no embedded packages, then there is no need to
            // populate defaults.
            return install;
        }

        // Expand any use of "all" in the defined embedded components.
        for component in install.components.iter_mut() {
            'embedded_package: for embedded_package in component.embedded.iter_mut() {
                if !(embedded_package.components().contains(&Component::All)) {
                    continue;
                }

                let mut matching_embedded = install
                    .embedded
                    .packages_matching_embedded_package(embedded_package);

                let Some(target_embedded_package) = matching_embedded.next() else {
                    continue;
                };

                for another_match in matching_embedded {
                    // If there are multiple embedded packages matching the
                    // embedded_package, then it is not possible to know
                    // which one to use to expand the "all" component.
                    //
                    // Unless they _all_ have identical component sets, then
                    // it isn't ambiguous.
                    if another_match.components().names()
                        != target_embedded_package.components().names()
                    {
                        continue 'embedded_package;
                    }
                }

                let new_components = target_embedded_package.components().names();
                if new_components.is_empty() {
                    // Empty components set? The embedded package is not allowed
                    // to have an empty component set.
                    continue;
                }

                embedded_package
                    .replace_all(new_components.into_iter().cloned())
                    .expect("new_components guaranteed to be non-empty");
            }
        }

        // If the same package is embedded multiple times, then it is not
        // possible to provide defaults.
        let mut embedded_names = std::collections::HashSet::new();
        for embedded in install.embedded.iter() {
            if !embedded_names.insert(embedded.name()) {
                return install;
            }
        }

        // Populate any missing components.embedded with default values.
        for component in install.components.iter_mut() {
            if !component.embedded.is_empty() {
                continue;
            }
            component.embedded = install
                .embedded
                .iter()
                .filter_map(|embedded| {
                    if embedded.components().names().contains(&component.name) {
                        Some(ComponentEmbeddedPackage::new(
                            OptVersionIdent::new(embedded.name().to_owned(), None),
                            component.name.clone(),
                        ))
                    } else {
                        None
                    }
                })
                .into();
            component.embedded.set_fabricated();
        }

        install
    }
}

struct RawInstallSpec {
    #[serde(default)]
    requirements: RequirementsList,
    #[serde(default)]
    embedded: EmbeddedPackagesList,
    #[serde(default)]
    components: ComponentSpecList,
    #[serde(default, deserialize_with = "deserialize_env_conf")]
    environment: EnvOpList,
}

impl<'de> Deserialize<'de> for RawInstallSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_map(RawInstallSpecVisitor::<RawInstallSpec>::default())
    }
}

impl<'de> Deserialize<'de> for LintedItem<RawInstallSpec> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_map(RawInstallSpecVisitor::<LintedItem<RawInstallSpec>>::default())
    }
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


impl<'de, D> serde::de::Visitor<'de> for RawInstallSpecVisitor<D>
where
    D: Default + From<RawInstallSpecVisitor<D>>,
{
    type Value = D;

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "requirements" => self.requirements = map.next_value::<RequirementsList>()?,
                "embedded" => self.embedded = map.next_value::<EmbeddedPackagesList>()?,
                "components" => self.components = map.next_value::<ComponentSpecList>()?,
                "environment" => self.environment = map.next_value::<LintedItem<EnvOpList>>()?,
                unknown_config => {
                    self.lints
                        .push(LintMessage::UnknownInstallSpecKey(InstallSpecKey::new(
                            unknown_config,
                        )));
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        Ok(self.into())
    }
}
