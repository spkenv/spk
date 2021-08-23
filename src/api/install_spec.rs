// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashSet;

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Build, BuildSpec, Ident, OptionMap, Request, Spec};
use crate::{Result, SpkError};

#[cfg(test)]
#[path = "./install_spec_test.rs"]
mod install_spec_test;

/// A set of structured installation parameters for a package.
#[pyclass]
#[derive(Debug, Default, Hash, Clone, PartialEq, Eq, Serialize)]
pub struct InstallSpec {
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<Request>,
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded: Vec<Spec>,
}

#[pymethods]
impl InstallSpec {
    /// Add or update a requirement to the set of installation requirements.
    ///
    /// If a request exists for the same name, it is replaced with the given
    /// one. Otherwise the new request is appended to the list.
    pub fn upsert_requirement(&mut self, request: Request) {
        let name = request.name();
        for other in self.requirements.iter_mut() {
            if other.name() == name {
                let _ = std::mem::replace(other, request);
                return;
            }
        }
        self.requirements.push(request);
    }
}

impl InstallSpec {
    pub fn is_empty(&self) -> bool {
        self.requirements.is_empty() && self.embedded.is_empty()
    }

    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<'a>(
        &mut self,
        options: &OptionMap,
        resolved: impl Iterator<Item = &'a Ident>,
    ) -> Result<()> {
        let mut by_name = std::collections::HashMap::new();
        for pkg in resolved {
            by_name.insert(pkg.name(), pkg);
        }
        for request in self.requirements.iter_mut() {
            match request {
                Request::Pkg(request) => {
                    if request.pin.is_none() {
                        continue;
                    }
                    match by_name.get(&request.pkg.name()) {
                        None => {
                            return Err(SpkError::InstallSpec(
                                format!("Cannot resolve fromBuildEnv, package not present: {}\nIs it missing from your package build options?", request.pkg.name())
                            ));
                        }
                        Some(resolved) => {
                            let rendered = request.render_pin(resolved)?;
                            let _ = std::mem::replace(request, rendered);
                        }
                    }
                }
                Request::Var(request) => {
                    if !request.pin {
                        continue;
                    }
                    let mut split = request.var.splitn(2, ".");
                    let (var, opts) = match (split.next().unwrap(), split.next()) {
                        (package, Some(var)) => (var, options.package_options(package)),
                        (var, None) => (var, options.clone()),
                    };
                    match opts.get(var) {
                        None => {
                            return Err(SpkError::InstallSpec(
                                format!("Cannot resolve fromBuildEnv, variable not set: {}\nIs it missing from the package build options?", request.var)
                            ));
                        }
                        Some(opt) => {
                            let rendered = request.render_pin(opt)?;
                            let _ = std::mem::replace(request, rendered);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for InstallSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Unchecked {
            #[serde(default)]
            requirements: Vec<Request>,
            #[serde(default)]
            embedded: Vec<Spec>,
        }

        let unchecked = Unchecked::deserialize(deserializer)?;
        let mut spec = InstallSpec {
            requirements: unchecked.requirements,
            embedded: unchecked.embedded,
        };

        let mut requirement_names = HashSet::with_capacity(spec.requirements.len());
        for name in spec.requirements.iter().map(Request::name) {
            if requirement_names.contains(&name) {
                return Err(serde::de::Error::custom(format!(
                    "found multiple install requirements for '{}'",
                    name
                )));
            }
            requirement_names.insert(name);
        }

        let mut default_build_spec = BuildSpec::default();
        for embedded in spec.embedded.iter_mut() {
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

        Ok(spec)
    }
}
