// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{OptVersionIdent, PinnableRequest, PinnedRequest};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::name::{OptName, PkgName};
use spk_schema_foundation::spec_ops::{HasBuildIdent, Named, Versioned};

use crate::component_embedded_packages::ComponentEmbeddedPackage;
use crate::foundation::option_map::OptionMap;
use crate::v0::EmbeddedRecipeSpec;
use crate::{
    ComponentSpec,
    ComponentSpecList,
    Components,
    EmbeddedPackagesList,
    EnvOp,
    EnvOpList,
    InstallSpec,
    OpKind,
    RecipeComponentSpec,
    RequirementsList,
    Result,
};

#[cfg(test)]
#[path = "./recipe_install_spec_test.rs"]
mod recipe_install_spec_test;

/// A set of structured installation parameters for a package.
///
/// This represents the `install` section of a package recipe. Once built,
/// [`crate::InstallSpec`] is used.
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
#[serde(from = "RawRecipeInstallSpec")]
pub struct RecipeInstallSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList<PinnableRequest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded: EmbeddedPackagesList<EmbeddedRecipeSpec>,
    #[serde(default)]
    pub components: ComponentSpecList<RecipeComponentSpec>,
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    pub environment: EnvOpList,
}

impl RecipeInstallSpec {
    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<K, R>(
        self,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<K, R>,
    ) -> Result<InstallSpec<PinnedRequest>>
    where
        K: Eq + std::hash::Hash,
        K: std::borrow::Borrow<PkgName>,
        R: HasBuildIdent + Versioned,
    {
        let RecipeInstallSpec {
            requirements,
            embedded,
            components,
            environment,
        } = self;

        let requirements = requirements.render_all_pins(options, resolved_by_name)?;
        let embedded = embedded.render_all_pins(options, resolved_by_name)?;
        let components = components.render_all_pins(options, resolved_by_name)?;

        Ok(InstallSpec {
            requirements,
            embedded,
            components,
            environment,
        })
    }
}

impl<Request> From<InstallSpec<Request>> for RecipeInstallSpec
where
    Request: DeserializeOwned + Named<OptName> + PartialEq + Serialize,
    RequirementsList: From<RequirementsList<Request>>,
    ComponentSpecList<RecipeComponentSpec>: From<ComponentSpecList<ComponentSpec>>,
{
    fn from(install: InstallSpec<Request>) -> Self {
        Self {
            requirements: install.requirements.into(),
            embedded: install.embedded.into(),
            components: install.components.into(),
            environment: install.environment,
        }
    }
}

impl From<RawRecipeInstallSpec> for RecipeInstallSpec {
    fn from(raw: RawRecipeInstallSpec) -> Self {
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
struct RawRecipeInstallSpec {
    #[serde(default)]
    requirements: RequirementsList<PinnableRequest>,
    #[serde(default)]
    embedded: EmbeddedPackagesList<EmbeddedRecipeSpec>,
    #[serde(default)]
    components: ComponentSpecList<RecipeComponentSpec>,
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
