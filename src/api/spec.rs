// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;
use std::path::Path;

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    request::is_false, Build, BuildSpec, Compat, Compatibility, Ident, Inheritance, InstallSpec,
    Opt, OptionMap, PkgRequest, Request, SourceSpec, TestSpec, VarRequest,
};
use crate::{Error, Result};

#[cfg(test)]
#[path = "./spec_test.rs"]
mod spec_test;

#[macro_export]
macro_rules! spec {
    ($($k:ident => $v:expr),* $(,)?) => {{
        use std::convert::TryInto;
        let mut spec = Spec::default();
        $(spec.$k = $v.try_into().unwrap();)*
        spec
    }};
}

#[pyclass]
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    #[serde(default)]
    pub pkg: Ident,
    #[serde(default, skip_serializing_if = "Compat::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceSpec>,
    #[serde(default, skip_serializing_if = "BuildSpec::is_default")]
    pub build: BuildSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<TestSpec>,
    #[serde(default, skip_serializing_if = "InstallSpec::is_empty")]
    pub install: InstallSpec,
}

impl Spec {
    /// Return the full set of resolved build options using the given ones.
    pub fn resolve_all_options(&self, given: &OptionMap) -> OptionMap {
        self.build
            .resolve_all_options(Some(&self.pkg.name()), given)
    }

    /// Check if this package spec satisfies the given request.
    pub fn sastisfies_request(&self, request: &Request) -> Compatibility {
        match request {
            Request::Pkg(request) => self.satisfies_pkg_request(&request),
            Request::Var(request) => self.satisfies_var_request(&request),
        }
    }

    /// Check if this package spec satisfies the given var request.
    pub fn satisfies_var_request(&self, request: &VarRequest) -> Compatibility {
        let opt_required = request.var.as_str() == self.pkg.name();
        let mut opt: Option<&Opt> = None;
        let request_name = &request.var;
        for o in self.build.options.iter() {
            if request_name == o.name() {
                opt = Some(o);
                break;
            }
            if request_name == &o.namespaced_name(self.pkg.name()) {
                opt = Some(o);
                break;
            }
        }

        match opt {
            None => {
                if opt_required {
                    return Compatibility::Incompatible(format!(
                        "Package does not define requested option: {}",
                        request.var
                    ));
                }
                return Compatibility::Compatible;
            }
            Some(Opt::Pkg(opt)) => opt.validate(Some(request.value())),
            Some(Opt::Var(opt)) => {
                let exact = opt.get_value(&Some(request.value().to_string()));
                if exact.as_ref().map(String::as_str) != Some(request.value()) {
                    Compatibility::Incompatible(format!(
                        "Incompatible build option '{}': {:?} != '{}'",
                        request.var,
                        exact,
                        request.value()
                    ))
                } else {
                    Compatibility::Compatible
                }
            }
        }
    }

    /// Check if this package spec satisfies the given pkg request.
    pub fn satisfies_pkg_request(&self, request: &PkgRequest) -> Compatibility {
        if request.pkg.name() != self.pkg.name() {
            return Compatibility::Incompatible(format!(
                "different package name: {} != {}",
                request.pkg.name(),
                self.pkg.name()
            ));
        }

        let compat = request.is_satisfied_by(self);
        if !compat.is_ok() {
            return compat;
        }

        if request.pkg.build.is_none() {
            return Compatibility::Compatible;
        }

        if request.pkg.build == self.pkg.build {
            return Compatibility::Compatible;
        }

        Compatibility::Incompatible(format!(
            "Package and request differ in builds: requested {:?}, got {:?}",
            request.pkg.build, self.pkg.build
        ))
    }

    /// Update this spec to represent a specific binary package build.
    pub fn update_for_build<'a>(
        &mut self,
        options: &OptionMap,
        resolved: impl Iterator<Item = &'a Spec>,
    ) -> Result<()> {
        let specs: HashMap<_, _> = resolved.map(|s| (s.pkg.name(), s)).collect();
        for (dep_name, dep_spec) in specs.iter() {
            for opt in dep_spec.build.options.iter() {
                if let Opt::Var(opt) = opt {
                    if let Inheritance::Weak = opt.inheritance {
                        continue;
                    }
                    let mut inherited_opt = opt.clone();
                    if !inherited_opt.var.contains(".") {
                        inherited_opt.var = format!("{}.{}", dep_name, opt.var);
                    }
                    inherited_opt.inheritance = Inheritance::Weak;
                    if let Inheritance::Strong = opt.inheritance {
                        let mut req = VarRequest::new(&inherited_opt.var);
                        req.pin = true;
                        self.install.upsert_requirement(Request::Var(req));
                    }
                    self.build.upsert_opt(Opt::Var(inherited_opt));
                }
            }
        }

        let mut build_options = self.build.options.clone();
        for e in self.install.embedded.iter() {
            build_options.extend(e.build.options.clone().into_iter());
        }

        for opt in build_options.iter_mut() {
            match opt {
                Opt::Var(opt) => {
                    opt.set_value(
                        options
                            .get(&opt.var)
                            .map(String::to_owned)
                            .or_else(|| opt.get_value(&None))
                            .unwrap_or_default(),
                    )?;
                    continue;
                }
                Opt::Pkg(opt) => {
                    let spec = specs.get(&opt.pkg.as_str());
                    match spec {
                        None => {
                            return Err(Error::String(format!(
                                "PkgOpt missing in resolved: {}",
                                opt.pkg
                            )));
                        }
                        Some(spec) => {
                            let rendered = spec.compat.render(&spec.pkg.version);
                            opt.set_value(rendered)?;
                        }
                    }
                }
            }
        }

        self.install
            .render_all_pins(options, specs.iter().map(|(_, s)| &s.pkg))?;
        let digest = self.resolve_all_options(options).digest();
        self.pkg.set_build(Some(Build::Digest(digest)));
        Ok(())
    }
}

/// ReadSpec loads a package specification from a yaml file.
pub fn read_spec_file<P: AsRef<Path>>(filepath: P) -> Result<Spec> {
    let file = std::fs::File::open(&filepath)?;
    let mut spec: Spec = serde_yaml::from_reader(file)?;
    if let Some(spec_root) = filepath.as_ref().parent() {
        for source in spec.sources.iter_mut() {
            if let SourceSpec::Local(source) = source {
                source.path = spec_root.join(&source.path);
            }
        }
    }

    Ok(spec)
}
