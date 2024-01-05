// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::spec_ops::Named;
use spk_schema_ident::{BuildIdent, OptVersionIdent};

use crate::component_embedded_packages::ComponentEmbeddedPackage;
use crate::foundation::option_map::OptionMap;
use crate::{
    ComponentSpecList,
    EmbeddedPackagesList,
    EnvOp,
    OpKind,
    Package,
    RequirementsList,
    Result,
};

#[cfg(test)]
#[path = "./install_spec_test.rs"]
mod install_spec_test;

/// A set of structured installation parameters for a package.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(from = "RawInstallSpec")]
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
            'embedded_package: for embedded_package in component.embedded_packages.iter_mut() {
                // Also expand if no components were explicitly specified.
                if !(embedded_package.components.is_empty()
                    || embedded_package.components.remove(&Component::All))
                {
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

                embedded_package.components.extend(
                    target_embedded_package
                        .components()
                        .names()
                        .into_iter()
                        .cloned(),
                );
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

        // Populate any missing components.embedded_packages with default
        // values.
        for component in install.components.iter_mut() {
            if !component.embedded_packages.is_empty() {
                continue;
            }
            component.embedded_packages = install
                .embedded
                .iter()
                .filter_map(|embedded| {
                    if embedded.components().names().contains(&component.name) {
                        Some(ComponentEmbeddedPackage {
                            pkg: OptVersionIdent::new(embedded.name().to_owned(), None),
                            components: [component.name.clone()].into(),
                        })
                    } else {
                        None
                    }
                })
                .into();
            component.embedded_packages.set_fabricated();
        }

        install
    }
}

/// A raw, unvalidated install spec.
#[derive(Deserialize)]
struct RawInstallSpec {
    #[serde(default)]
    requirements: RequirementsList,
    #[serde(default)]
    embedded: EmbeddedPackagesList,
    #[serde(default)]
    components: ComponentSpecList,
    #[serde(default, deserialize_with = "deserialize_env_conf")]
    environment: Vec<EnvOp>,
}

fn deserialize_env_conf<'de, D>(deserializer: D) -> std::result::Result<Vec<EnvOp>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct EnvConfVisitor;

    impl<'de> serde::de::Visitor<'de> for EnvConfVisitor {
        type Value = Vec<EnvOp>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an environment configuration")
        }

        fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();

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
