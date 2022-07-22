// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use serde::{Deserialize, Serialize};

use super::{build::EmbeddedSource, Build, BuildSpec, InstallSpec, Spec};

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
        let mut unchecked = Vec::<super::v0::Spec>::deserialize(deserializer)?;

        let mut default_build_spec = BuildSpec::default();
        let mut default_install_spec = InstallSpec::default();
        for embedded in unchecked.iter_mut() {
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
                Some(Build::Embedded(EmbeddedSource::Unknown)) => continue,
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
        }

        Ok(EmbeddedPackagesList(
            unchecked.into_iter().map(From::from).collect(),
        ))
    }
}
