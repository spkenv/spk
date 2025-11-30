// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{OptVersionIdent, PinnedRequest};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::name::OptName;
use spk_schema_foundation::spec_ops::Named;

use crate::component_embedded_packages::ComponentEmbeddedPackage;
use crate::v0::{EmbeddedInstallSpec, EmbeddedPackageSpec};
use crate::{
    ComponentSpec,
    ComponentSpecList,
    Components,
    EmbeddedPackagesList,
    EnvOp,
    EnvOpList,
    OpKind,
    RequirementsList,
};

#[cfg(test)]
#[path = "./install_spec_test.rs"]
mod install_spec_test;

/// A set of structured installation parameters for a package.
///
/// This represents the `install` section of a built package. See
/// [`crate::v0::RecipeInstallSpec`] for the type used by recipes.
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
    from = "RawInstallSpec<Request>",
    bound = "Request: DeserializeOwned + Named<OptName>"
)]
pub struct InstallSpec<Request: DeserializeOwned + Named<OptName> + PartialEq + Serialize> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList<Request>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded: EmbeddedPackagesList<EmbeddedPackageSpec>,
    #[serde(default)]
    pub components: ComponentSpecList<ComponentSpec>,
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    pub environment: EnvOpList,
}

impl<Request> Default for InstallSpec<Request>
where
    Request: DeserializeOwned + Named<OptName> + PartialEq + Serialize,
{
    fn default() -> Self {
        Self {
            requirements: RequirementsList::default(),
            embedded: EmbeddedPackagesList::default(),
            components: ComponentSpecList::default(),
            environment: EnvOpList::default(),
        }
    }
}

impl From<EmbeddedInstallSpec> for InstallSpec<PinnedRequest> {
    fn from(embedded: EmbeddedInstallSpec) -> Self {
        Self {
            requirements: embedded.requirements,
            embedded: EmbeddedPackagesList::default(),
            components: embedded.components,
            environment: embedded.environment,
        }
    }
}

impl<Request> From<RawInstallSpec<Request>> for InstallSpec<Request>
where
    Request: DeserializeOwned + Named<OptName> + PartialEq + Serialize,
{
    fn from(raw: RawInstallSpec<Request>) -> Self {
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

/// A raw, unvalidated install spec.
#[derive(Deserialize)]
#[serde(bound = "Request: DeserializeOwned + Named<OptName> + Serialize")]
struct RawInstallSpec<Request> {
    #[serde(default)]
    requirements: RequirementsList<Request>,
    #[serde(default)]
    embedded: EmbeddedPackagesList<EmbeddedPackageSpec>,
    #[serde(default)]
    components: ComponentSpecList<ComponentSpec>,
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
