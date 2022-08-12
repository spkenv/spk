// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::api::{
    request::is_false, Build, BuildSpec, Compat, Compatibility, Ident, Inheritance, InstallSpec,
    Meta, Opt, OptionMap, Package, PkgRequest, Recipe, Request, SourceSpec, TestSpec, VarRequest,
};
use crate::api::{Named, Versioned};
use crate::{api, Error, Result};

#[cfg(test)]
#[path = "./spec_test.rs"]
mod spec_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct Spec {
    pub pkg: Ident,
    #[serde(default, skip_serializing_if = "Meta::is_default")]
    pub meta: Meta,
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
    #[serde(default, skip_serializing_if = "InstallSpec::is_default")]
    pub install: InstallSpec,
}

impl Spec {
    /// Create an empty spec for the identified package
    pub fn new(ident: Ident) -> Self {
        Self {
            pkg: ident,
            meta: Meta::default(),
            compat: Compat::default(),
            deprecated: bool::default(),
            sources: Vec::new(),
            build: BuildSpec::default(),
            tests: Vec::new(),
            install: InstallSpec::default(),
        }
    }

    /// Check if this package spec satisfies the given request.
    pub fn satisfies_request(&self, request: Request) -> Compatibility {
        match request {
            Request::Pkg(request) => self.satisfies_pkg_request(&request),
            Request::Var(request) => self.satisfies_var_request(&request),
        }
    }

    /// Check if this package spec satisfies the given var request.
    pub fn satisfies_var_request(&self, request: &VarRequest) -> Compatibility {
        let opt_required = request.var.namespace() == Some(&self.pkg.name);
        let mut opt: Option<&Opt> = None;
        for o in self.build.options.iter() {
            let is_same_base_name = request.var.base_name() == o.base_name();
            if !is_same_base_name {
                continue;
            }

            let is_global = request.var.namespace().is_none();
            let is_this_namespace = request.var.namespace() == Some(&*self.pkg.name);
            if is_this_namespace || is_global {
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
                Compatibility::Compatible
            }
            Some(Opt::Pkg(opt)) => opt.validate(Some(&request.value)),
            Some(Opt::Var(opt)) => {
                let exact = opt.get_value(Some(&request.value));
                if exact.as_deref() != Some(&request.value) {
                    Compatibility::Incompatible(format!(
                        "Incompatible build option '{}': has '{}', requires '{}'",
                        request.var,
                        exact.unwrap_or_else(|| "None".to_string()),
                        request.value,
                    ))
                } else {
                    Compatibility::Compatible
                }
            }
        }
    }

    /// Check if this package spec satisfies the given pkg request.
    pub fn satisfies_pkg_request(&self, request: &PkgRequest) -> Compatibility {
        if request.pkg.name != self.pkg.name {
            return Compatibility::Incompatible(format!(
                "different package name: {} != {}",
                request.pkg.name, self.pkg.name
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
}

impl Named for Spec {
    fn name(&self) -> &api::PkgName {
        &self.pkg.name
    }
}

impl Versioned for Spec {
    fn version(&self) -> &api::Version {
        &self.pkg.version
    }
}

impl api::Deprecate for Spec {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl api::DeprecateMut for Spec {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl Package for Spec {
    fn ident(&self) -> &Ident {
        &self.pkg
    }

    fn compat(&self) -> &api::Compat {
        &self.compat
    }

    fn option_values(&self) -> api::OptionMap {
        let mut opts = api::OptionMap::default();
        for opt in self.build.options.iter() {
            // we are assuming that this spec has been updated to represent
            // a build and had all of the options pinned/resolved.
            opts.insert(opt.full_name().to_owned(), opt.get_value(None));
        }
        opts
    }

    fn options(&self) -> &Vec<api::Opt> {
        &self.build.options
    }

    fn sources(&self) -> &Vec<api::SourceSpec> {
        &self.sources
    }

    fn embedded(&self) -> &api::EmbeddedPackagesList {
        &self.install.embedded
    }

    fn components(&self) -> &api::ComponentSpecList {
        &self.install.components
    }

    fn runtime_environment(&self) -> &Vec<api::EnvOp> {
        &self.install.environment
    }

    fn runtime_requirements(&self) -> &api::RequirementsList {
        &self.install.requirements
    }

    fn validation(&self) -> &api::ValidationSpec {
        &self.build.validation
    }

    fn build_script(&self) -> String {
        self.build.script.join("\n")
    }
}

impl Recipe for Spec {
    type Output = Self;

    fn default_variants(&self) -> &Vec<OptionMap> {
        &self.build.variants
    }

    fn resolve_options(&self, given: &api::OptionMap) -> Result<api::OptionMap> {
        let mut resolved = api::OptionMap::default();
        for opt in self.options().iter() {
            let given_value = match opt.full_name().namespace() {
                Some(_) => given
                    .get(opt.full_name())
                    .or_else(|| given.get(opt.full_name().without_namespace())),
                None => given
                    .get(&opt.full_name().with_namespace(self.name()))
                    .or_else(|| given.get(opt.full_name())),
            };
            let value = opt.get_value(given_value.map(String::as_ref));
            resolved.insert(opt.full_name().to_owned(), value);
        }

        let compat = self.validate_options(&resolved);
        if !compat.is_ok() {
            return Err(Error::String(compat.to_string()));
        }
        Ok(resolved)
    }

    fn get_build_requirements(&self, options: &api::OptionMap) -> Result<Vec<api::Request>> {
        let mut requests = Vec::new();
        for opt in self.options().iter() {
            match opt {
                api::Opt::Pkg(opt) => {
                    let given_value = options.get(opt.pkg.as_opt_name()).map(String::to_owned);
                    let mut req = opt.to_request(
                        given_value,
                        api::RequestedBy::BinaryBuild(Recipe::to_ident(self)),
                    )?;
                    if req.pkg.components.is_empty() {
                        // inject the default component for this context if needed
                        req.pkg.components.insert(api::Component::Build);
                    }
                    requests.push(req.into());
                }
                api::Opt::Var(opt) => {
                    // If no value was specified in the spec, there's
                    // no need to turn that into a requirement to
                    // find a var with an empty value.
                    if let Some(value) = options.get(&opt.var) {
                        if !value.is_empty() {
                            requests.push(opt.to_request(Some(value)).into());
                        }
                    }
                }
            }
        }
        Ok(requests)
    }

    fn get_tests(&self, _options: &api::OptionMap) -> Result<Vec<api::TestSpec>> {
        Ok(self.tests.clone())
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self> {
        // TODO: remove all things that would cause this to not resolve
        //       after solver no longer treats source packages differently
        let mut source = self.clone();
        source.pkg.set_build(Some(Build::Source));
        for source in source.sources.iter_mut() {
            if let api::SourceSpec::Local(source) = source {
                source.path = root.join(&source.path);
            }
        }
        Ok(source)
    }

    fn generate_binary_build(
        &self,
        options: &OptionMap,
        build_env: &crate::solve::Solution,
    ) -> Result<Self> {
        let mut updated = self.clone();
        let specs: HashMap<_, _> = build_env
            .items()
            .into_iter()
            .map(|s| (s.spec.name().to_owned(), s.spec))
            .collect();
        for (dep_name, dep_spec) in specs.iter() {
            for opt in dep_spec.options().iter() {
                if let Opt::Var(opt) = opt {
                    if let Inheritance::Weak = opt.inheritance {
                        continue;
                    }
                    let mut inherited_opt = opt.clone();
                    if inherited_opt.var.namespace().is_none() {
                        inherited_opt.var = inherited_opt.var.with_namespace(&dep_name);
                    }
                    inherited_opt.inheritance = Inheritance::Weak;
                    if let Inheritance::Strong = opt.inheritance {
                        let mut req = VarRequest::new(inherited_opt.var.clone());
                        req.pin = true;
                        updated.install.upsert_requirement(Request::Var(req));
                    }
                    updated.build.upsert_opt(Opt::Var(inherited_opt));
                }
            }
        }

        for e in updated.install.embedded.iter() {
            updated
                .build
                .options
                .extend(e.options().clone().into_iter());
        }

        for opt in updated.build.options.iter_mut() {
            match opt {
                Opt::Var(opt) => {
                    opt.set_value(
                        options
                            .get(&opt.var)
                            .or_else(|| options.get(opt.var.without_namespace()))
                            .map(String::to_owned)
                            .or_else(|| opt.get_value(None))
                            .unwrap_or_default(),
                    )?;
                    continue;
                }
                Opt::Pkg(opt) => {
                    let spec = specs.get(&opt.pkg);
                    match spec {
                        None => {
                            return Err(Error::String(format!(
                                "PkgOpt missing in resolved: {}",
                                opt.pkg
                            )));
                        }
                        Some(spec) => {
                            let rendered = spec.compat().render(spec.version());
                            opt.set_value(rendered)?;
                        }
                    }
                }
            }
        }

        updated
            .install
            .render_all_pins(options, specs.iter().map(|(_, s)| s.as_ref().ident()))?;
        let digest = updated.resolve_options(options)?.digest();
        updated.pkg.set_build(Some(Build::Digest(digest)));
        Ok(updated)
    }
}

impl<'de> Deserialize<'de> for Spec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SpecSchema {
            pkg: Ident,
            #[serde(default)]
            meta: Meta,
            #[serde(default)]
            compat: Compat,
            #[serde(default)]
            deprecated: bool,
            #[serde(default)]
            sources: Option<Vec<SourceSpec>>,
            #[serde(default)]
            build: serde_yaml::Mapping,
            #[serde(default)]
            tests: Vec<TestSpec>,
            #[serde(default)]
            install: InstallSpec,
        }
        let unchecked = SpecSchema::deserialize(deserializer)?;
        let build_spec_result = if unchecked.pkg.build.is_none() {
            BuildSpec::deserialize(serde_yaml::Value::Mapping(unchecked.build))
        } else {
            // if the build is set, we assume that this is a rendered spec
            // and we do not want to make an existing rendered build spec unloadable
            BuildSpec::deserialize_unsafe(serde_yaml::Value::Mapping(unchecked.build))
        };
        let build_spec = build_spec_result
            .map_err(|err| serde::de::Error::custom(format!("spec.build: {err}")))?;

        Ok(Spec {
            pkg: unchecked.pkg,
            meta: unchecked.meta,
            compat: unchecked.compat,
            deprecated: unchecked.deprecated,
            sources: unchecked
                .sources
                .unwrap_or_else(|| vec![SourceSpec::Local(api::LocalSource::default())]),
            build: build_spec,
            tests: unchecked.tests,
            install: unchecked.install,
        })
    }
}
