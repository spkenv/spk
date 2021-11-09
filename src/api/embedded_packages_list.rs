// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Build, BuildSpec, Spec};

#[cfg(test)]
#[path = "./embedded_packages_list_test.rs"]
mod embedded_packages_list_test;

/// A set of packages that are embedded/provided by another.
#[derive(Debug, Default, Hash, Clone, PartialEq, Eq, Serialize)]
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
        let mut unchecked = Vec::<Spec>::deserialize(deserializer)?;

        let mut default_build_spec = BuildSpec::default();
        for embedded in unchecked.iter_mut() {
            default_build_spec.options = embedded.build.options.clone();
            if default_build_spec != embedded.build {
                return Err(serde::de::Error::custom(
                    "embedded packages can only specify build.options",
                ));
            }
            if !embedded.install.is_empty() {
                return Err(serde::de::Error::custom(
                    "embedded packages cannot specify the install field",
                ));
            }
            match &mut embedded.pkg.build {
                Some(Build::Embedded) => continue,
                None => embedded.pkg.set_build(Some(Build::Embedded)),
                Some(_) => {
                    return Err(serde::de::Error::custom(format!(
                        "embedded package should not specify a build, got: {}",
                        embedded.pkg
                    )));
                }
            }
        }

        Ok(EmbeddedPackagesList(unchecked))
    }
}

impl IntoPy<Py<pyo3::types::PyAny>> for EmbeddedPackagesList {
    fn into_py(self, py: Python) -> Py<pyo3::types::PyAny> {
        self.0.into_py(py)
    }
}

impl<'source> FromPyObject<'source> for EmbeddedPackagesList {
    fn extract(ob: &'source PyAny) -> PyResult<Self> {
        Ok(EmbeddedPackagesList(Vec::<Spec>::extract(ob)?))
    }
}
