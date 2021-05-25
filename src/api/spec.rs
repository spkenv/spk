// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    request::is_false, Build, BuildSpec, Compat, Compatibility, Ident, Inheritance, Opt, OptionMap,
    PkgRequest, Request, SourceSpec, TestSpec, VarRequest,
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

/// A set of structured installation parameters for a package.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct InstallSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    requirements: Vec<Request>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    embedded: Vec<Spec>,
}

impl InstallSpec {
    pub fn is_empty(&self) -> bool {
        self.requirements.is_empty() && self.embedded.is_empty()
    }

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
                            return Err(Error::String(
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
                            return Err(Error::String(
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
        let spec = InstallSpec {
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
        for embedded in spec.embedded.iter() {
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
            if let Some(_) = embedded.pkg.build {
                return Err(serde::de::Error::custom(format!(
                    "embedded package should not specify a build, got: {}",
                    embedded.pkg
                )));
            }
        }

        Ok(spec)
    }
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
