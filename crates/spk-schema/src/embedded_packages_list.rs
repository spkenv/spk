// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_build::EmbeddedSource;

use super::{BuildSpec, InstallSpec, Spec};
use crate::foundation::ident_build::Build;

#[cfg(test)]
#[path = "./embedded_packages_list_test.rs"]
mod embedded_packages_list_test;

/// A set of packages that are embedded/provided by another.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EmbeddedPackagesList(Vec<Spec>);

impl std::ops::Deref for EmbeddedPackagesList {
    type Target = Vec<Spec>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for EmbeddedPackagesList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de> Deserialize<'de> for EmbeddedPackagesList {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct EmbeddedPackagesListVisitor;

        impl<'de> serde::de::Visitor<'de> for EmbeddedPackagesListVisitor {
            type Value = EmbeddedPackagesList;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of embedded packages")
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let size_hint = seq.size_hint().unwrap_or(0);
                let mut embedded_stubs = Vec::with_capacity(size_hint);
                let mut default_build_spec = BuildSpec::default();
                let mut default_install_spec = InstallSpec::default();
                while let Some(mut embedded) = seq.next_element::<super::v0::Spec>()? {
                    default_build_spec.options = embedded.build.options.clone();
                    if default_build_spec != embedded.build {
                        return Err(serde::de::Error::custom(
                            "embedded packages can only specify build.options",
                        ));
                    }
                    default_install_spec.components = embedded.install.components.clone();
                    if default_install_spec != embedded.install {
                        return Err(serde::de::Error::custom(
                            "embedded packages can only specify install.components",
                        ));
                    }
                    match &mut embedded.pkg.build {
                        Some(Build::Embedded(EmbeddedSource::Unknown)) => {}
                        None => embedded
                            .pkg
                            .set_build(Some(Build::Embedded(EmbeddedSource::Unknown))),
                        Some(_) => {
                            return Err(serde::de::Error::custom(format!(
                                "embedded package should not specify a build, got: {}",
                                embedded.pkg
                            )));
                        }
                    }
                    embedded_stubs.push(Spec::V0Package(embedded));
                }
                Ok(EmbeddedPackagesList(embedded_stubs))
            }
        }

        deserializer.deserialize_seq(EmbeddedPackagesListVisitor)
    }
}
