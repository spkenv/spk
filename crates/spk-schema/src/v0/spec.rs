// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{BTreeSet, HashMap};
use std::convert::TryInto;
use std::path::Path;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::foundation::ident_build::Build;
use crate::foundation::ident_component::Component;
use crate::foundation::name::PkgName;
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::{Named, PackageOps, RecipeOps, Versioned};
use crate::foundation::version::{Compat, CompatRule, Compatibility, Version};
use crate::foundation::version_range::Ranged;
use crate::ident::{
    is_false, Ident, PkgRequest, PreReleasePolicy, RangeIdent, Request, RequestedBy, VarRequest,
};
use crate::BuildEnv;
use crate::{
    meta::Meta, test_spec::TestSpec, BuildSpec, ComponentSpec, ComponentSpecList, Deprecate,
    DeprecateMut, EmbeddedPackagesList, EnvOp, Error, Inheritance, InstallSpec, LocalSource, Opt,
    Package, Recipe, RequirementsList, Result, SourceSpec, ValidationSpec,
};

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

    /// Remove requirements and other package data that
    /// is not relevant for a source package build.
    fn prune_for_source_build(&mut self) {
        self.install.requirements.clear();
        self.build = Default::default();
        self.tests.clear();
        self.install.components.clear();
        self.install.components.push(ComponentSpec {
            name: Component::Source,
            files: Default::default(),
            uses: Default::default(),
            requirements: Default::default(),
            embedded: Default::default(),
        });
    }
}

impl Named for Spec {
    fn name(&self) -> &PkgName {
        &self.pkg.name
    }
}

impl Versioned for Spec {
    fn version(&self) -> &Version {
        &self.pkg.version
    }
}

impl Deprecate for Spec {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl DeprecateMut for Spec {
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
    type Input = Self;

    fn compat(&self) -> &Compat {
        &self.compat
    }

    fn option_values(&self) -> OptionMap {
        let mut opts = OptionMap::default();
        for opt in self.build.options.iter() {
            // we are assuming that this spec has been updated to represent
            // a build and had all of the options pinned/resolved.
            opts.insert(opt.full_name().to_owned(), opt.get_value(None));
        }
        opts
    }

    fn options(&self) -> &Vec<Opt> {
        &self.build.options
    }

    fn sources(&self) -> &Vec<SourceSpec> {
        &self.sources
    }

    fn embedded(&self) -> &EmbeddedPackagesList {
        &self.install.embedded
    }

    fn embedded_as_recipes(
        &self,
    ) -> std::result::Result<Vec<(Self::Input, Option<Component>)>, &str> {
        self.install
            .embedded
            .iter()
            .map(|embed| (embed.clone(), None))
            .chain(self.install.components.iter().flat_map(|cs| {
                cs.embedded
                    .iter()
                    .map(move |embed| (embed.clone(), Some(cs.name.clone())))
            }))
            .map(|(recipe, component)| recipe.try_into().map(|r| (r, component)))
            .collect()
    }

    fn components(&self) -> &ComponentSpecList {
        &self.install.components
    }

    fn runtime_environment(&self) -> &Vec<EnvOp> {
        &self.install.environment
    }

    fn runtime_requirements(&self) -> &RequirementsList {
        &self.install.requirements
    }

    fn validation(&self) -> &ValidationSpec {
        &self.build.validation
    }

    fn build_script(&self) -> String {
        self.build.script.join("\n")
    }
}

impl Recipe for Spec {
    type Output = Self;
    type Recipe = Self;

    fn default_variants(&self) -> &Vec<OptionMap> {
        &self.build.variants
    }

    fn resolve_options(&self, given: &OptionMap) -> Result<OptionMap> {
        let mut resolved = OptionMap::default();
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

    fn get_build_requirements(&self, options: &OptionMap) -> Result<Vec<Request>> {
        let mut requests = Vec::new();
        for opt in self.options().iter() {
            match opt {
                Opt::Pkg(opt) => {
                    let given_value = options.get(opt.pkg.as_opt_name()).map(String::to_owned);
                    let mut req =
                        opt.to_request(given_value, RequestedBy::BinaryBuild(self.to_ident()))?;
                    if req.pkg.components.is_empty() {
                        // inject the default component for this context if needed
                        req.pkg.components.insert(Component::default_for_build());
                    }
                    requests.push(req.into());
                }
                Opt::Var(opt) => {
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

    fn get_tests(&self, _options: &OptionMap) -> Result<Vec<TestSpec>> {
        Ok(self.tests.clone())
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self> {
        let mut source = self.clone();
        source.pkg.set_build(Some(Build::Source));
        source.prune_for_source_build();
        for source in source.sources.iter_mut() {
            if let SourceSpec::Local(source) = source {
                source.path = root.join(&source.path);
            }
        }
        Ok(source)
    }

    fn generate_binary_build<E, P>(&self, options: &OptionMap, build_env: &E) -> Result<Self>
    where
        E: BuildEnv<Package = P>,
        P: Package<Ident = Ident>,
    {
        let mut updated = self.clone();
        let specs: HashMap<_, _> = build_env
            .build_env()
            .into_iter()
            .map(|p| (p.name().to_owned(), p))
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
                            let rendered = spec.compat().render(Versioned::version(spec));
                            opt.set_value(rendered)?;
                        }
                    }
                }
            }
        }

        updated
            .install
            .render_all_pins(options, specs.iter().map(|(_, p)| p.ident()))?;
        let digest = updated.resolve_options(options)?.digest();
        updated.pkg.set_build(Some(Build::Digest(digest)));
        Ok(updated)
    }

    fn with_build(&self, build: Option<Build>) -> Self {
        let mut r = self.clone();
        r.pkg.build = build;
        r
    }
}

impl RecipeOps for Spec {
    type Ident = Ident;
    type PkgRequest = PkgRequest;
    type RangeIdent = RangeIdent;

    fn is_api_compatible(&self, base: &crate::foundation::version::Version) -> Compatibility {
        self.compat()
            .is_api_compatible(base, Versioned::version(&self))
    }

    fn is_binary_compatible(&self, base: &crate::foundation::version::Version) -> Compatibility {
        self.compat()
            .is_binary_compatible(base, Versioned::version(&self))
    }

    fn is_satisfied_by_range_ident(
        &self,
        range_ident: &RangeIdent,
        required: crate::foundation::version::CompatRule,
    ) -> Compatibility {
        if self.name() != range_ident.name {
            return Compatibility::Incompatible("different package names".into());
        }
        let source_package_requested = range_ident.build == Some(Build::Source);
        let is_source_build = self.ident().is_source() && !source_package_requested;
        if !range_ident.components.is_empty() && !is_source_build {
            let required_components = self
                .components()
                .resolve_uses(range_ident.components.iter());
            let available_components: BTreeSet<_> =
                self.components_iter().map(|c| c.name.clone()).collect();
            let missing_components = required_components
                .difference(&available_components)
                .sorted()
                .collect_vec();
            if !missing_components.is_empty() {
                return Compatibility::Incompatible(format!(
                    "does not define requested components: [{}], found [{}]",
                    missing_components
                        .into_iter()
                        .map(Component::to_string)
                        .join(", "),
                    available_components
                        .iter()
                        .map(Component::to_string)
                        .sorted()
                        .join(", ")
                ));
            }
        }

        let c = range_ident.version.is_satisfied_by(self, required);
        if !c.is_ok() {
            return c;
        }

        if range_ident.build.is_some() && range_ident.build != self.ident().build {
            return Compatibility::Incompatible(format!(
                "requested build {:?} != {:?}",
                range_ident.build,
                self.ident().build
            ));
        }

        Compatibility::Compatible
    }

    fn is_satisfied_by_pkg_request(&self, pkg_request: &PkgRequest) -> Compatibility {
        if self.is_deprecated() {
            // deprecated builds are only okay if their build
            // was specifically requested
            if pkg_request.pkg.build.is_none() || pkg_request.pkg.build != self.ident().build {
                return Compatibility::Incompatible(
                    "Build is deprecated and was not specifically requested".to_string(),
                );
            }
        }

        if pkg_request.prerelease_policy == PreReleasePolicy::ExcludeAll
            && !Versioned::version(&self).pre.is_empty()
        {
            return Compatibility::Incompatible("prereleases not allowed".to_string());
        }

        pkg_request.pkg.is_satisfied_by(
            self,
            pkg_request.required_compat.unwrap_or(CompatRule::Binary),
        )
    }

    fn to_ident(&self) -> Self::Ident {
        Self::Ident {
            name: self.name().to_owned(),
            version: self.version().clone(),
            build: None,
        }
    }
}

impl PackageOps for Spec {
    type Ident = Ident;
    type Component = ComponentSpec;
    type VarRequest = VarRequest;

    fn components_iter(&self) -> std::slice::Iter<'_, Self::Component> {
        self.install.components.iter()
    }

    fn ident(&self) -> &Self::Ident {
        &self.pkg
    }

    fn is_satisfied_by_var_request(&self, var_request: &VarRequest) -> Compatibility {
        let opt_required = var_request.var.namespace() == Some(self.name());
        let mut opt: Option<&Opt> = None;
        let request_name = &var_request.var;
        for o in self.options().iter() {
            if request_name == o.full_name() {
                opt = Some(o);
                break;
            }
            if request_name == &o.full_name().with_namespace(self.name()) {
                opt = Some(o);
                break;
            }
        }

        match opt {
            None => {
                if opt_required {
                    return Compatibility::Incompatible(format!(
                        "Package does not define requested option: {}",
                        var_request.var
                    ));
                }
                Compatibility::Compatible
            }
            Some(Opt::Pkg(opt)) => opt.validate(Some(&var_request.value)),
            Some(Opt::Var(opt)) => {
                let exact = opt.get_value(Some(&var_request.value));
                if exact.as_deref() != Some(&var_request.value) {
                    Compatibility::Incompatible(format!(
                        "Incompatible build option '{}': '{}' != '{}'",
                        var_request.var,
                        exact.unwrap_or_else(|| "None".to_string()),
                        var_request.value
                    ))
                } else {
                    Compatibility::Compatible
                }
            }
        }
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

        let mut spec = Spec {
            pkg: unchecked.pkg,
            meta: unchecked.meta,
            compat: unchecked.compat,
            deprecated: unchecked.deprecated,
            sources: unchecked
                .sources
                .unwrap_or_else(|| vec![SourceSpec::Local(LocalSource::default())]),
            build: build_spec,
            tests: unchecked.tests,
            install: unchecked.install,
        };
        if spec.pkg.is_source() {
            // for backward-compatibility with older publishes, prune out anything
            // that is not relevant to a source package, since now source packages
            // can technically have their own requirements, etc.
            spec.prune_for_source_build();
        }
        Ok(spec)
    }
}
