use std::borrow::Cow;
// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{BTreeSet, HashMap};
use std::convert::TryInto;
use std::path::Path;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::name::PkgNameBuf;
use spk_schema_foundation::option_map::Stringified;
use spk_schema_ident::{AnyIdent, BuildIdent, Ident, VersionIdent};

use crate::foundation::ident_build::Build;
use crate::foundation::ident_component::Component;
use crate::foundation::name::PkgName;
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, CompatRule, Compatibility, Version};
use crate::foundation::version_range::Ranged;
use crate::ident::{
    is_false,
    PkgRequest,
    PreReleasePolicy,
    Request,
    RequestedBy,
    Satisfy,
    VarRequest,
};
use crate::meta::Meta;
use crate::v0::{BuildSpec, Inheritance, InstallSpec, Opt, TestSpec, UncheckedBuildSpec};
use crate::{
    BuildEnv,
    BuildEnvMember,
    ComponentSpec,
    Deprecate,
    DeprecateMut,
    EnvOp,
    Error,
    LocalSource,
    Package,
    PackageMut,
    Recipe,
    RequirementsList,
    Result,
    SourceSpec,
    TestStage,
    ValidationSpec,
};

#[cfg(test)]
#[path = "./spec_test.rs"]
mod spec_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct Spec<Ident> {
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

impl<Ident> Spec<Ident> {
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

    /// Convert the ident type associated to this package
    pub fn map_ident<F, ToIdent>(self, map: F) -> Spec<ToIdent>
    where
        F: FnOnce(Ident) -> ToIdent,
    {
        Spec {
            pkg: map(self.pkg),
            meta: self.meta,
            compat: self.compat,
            deprecated: self.deprecated,
            sources: self.sources,
            build: self.build,
            tests: self.tests,
            install: self.install,
        }
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
            file_match_mode: Default::default(),
        });
    }
}

impl Spec<BuildIdent> {
    /// Check if this package spec satisfies the given request.
    pub fn satisfies_request(&self, request: Request) -> Compatibility {
        match request {
            Request::Pkg(request) => Satisfy::check_satisfies_request(self, &request),
            Request::Var(request) => Satisfy::check_satisfies_request(self, &request),
        }
    }
}

impl<Ident: Named> Named for Spec<Ident> {
    fn name(&self) -> &PkgName {
        self.pkg.name()
    }
}

impl<Ident: HasVersion> HasVersion for Spec<Ident> {
    fn version(&self) -> &Version {
        self.pkg.version()
    }
}

impl<Ident: HasVersion> Versioned for Spec<Ident> {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl<Ident> Deprecate for Spec<Ident> {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl<Ident> DeprecateMut for Spec<Ident> {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl Package for Spec<BuildIdent> {
    type EmbeddedStub = Self;

    fn ident(&self) -> &BuildIdent {
        &self.pkg
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

    fn sources(&self) -> &Vec<SourceSpec> {
        &self.sources
    }

    fn embedded<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Vec<Self::EmbeddedStub> {
        let mut base: Vec<_> = self
            .install
            .embedded
            .iter()
            .cloned()
            .map(|e| e.into_inner())
            .collect();
        let components: BTreeSet<_> = components.into_iter().collect();
        for component in self.install.components.iter() {
            if components.contains(&component.name) {
                base.extend(component.embedded.iter().cloned().map(|e| e.into_inner()));
            }
        }
        base
    }

    fn components(&self) -> Cow<'_, crate::ComponentSpecList<Self::EmbeddedStub>> {
        Cow::Owned(
            self.install
                .components
                .clone()
                .map_embedded_stubs(|e| e.into_inner()),
        )
    }

    fn runtime_environment(&self) -> &Vec<EnvOp> {
        &self.install.environment
    }

    fn runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        let mut requirements = Cow::Borrowed(&self.install.requirements);
        let components = self.install.components.resolve_uses(components);
        let mut additional: Vec<_> = components.flat_map(|c| c.requirements.clone()).collect();
        if !additional.is_empty() {
            // only trigger a copy of the borrowed data if there's actually
            // more requests to be appended
            requirements.to_mut().append(&mut additional);
        }
        requirements
    }

    fn downstream_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        let requests = self
            .build
            .options
            .iter()
            .filter_map(|opt| match opt {
                Opt::Var(v) => Some(v),
                Opt::Pkg(_) => None,
            })
            .filter(|o| {
                // the original "StrongForBuildOnly" variant was being injected
                // but didn't actually have any effect on resolved build environments
                // so we maintain the current behavior by only recognizing strongly
                // inherited options
                o.inheritance == Inheritance::Strong
            })
            .map(|o| {
                let var = o.var.with_default_namespace(self.name());
                VarRequest {
                    var,
                    // we are assuming that the var here will have a value because
                    // this is a built binary package
                    value: o.get_value(None).unwrap_or_default(),
                    pin: false,
                }
            })
            .map(Request::Var)
            .collect();
        Cow::Owned(requests)
    }

    fn validation(&self) -> &ValidationSpec {
        &self.build.validation
    }

    fn build_script(&self) -> Cow<'_, String> {
        Cow::Owned(self.build.script.join("\n"))
    }

    fn validate_options(&self, given_options: &OptionMap) -> Compatibility {
        let mut must_exist = given_options.package_options_without_global(self.name());
        let given_options = given_options.package_options(self.name());
        for option in self.build.options.iter() {
            let value = given_options
                .get_for_package(self.pkg.name(), option.full_name())
                .map(String::as_str);
            let compat = option.validate(value);
            if !compat.is_ok() {
                return Compatibility::incompatible(format!(
                    "invalid value for {}: {compat}",
                    option.full_name(),
                ));
            }

            must_exist.remove(option.full_name().without_namespace());
        }

        if !must_exist.is_empty() {
            let missing = must_exist;
            return Compatibility::incompatible(format!(
                "Package does not define requested build options: {missing:?}",
            ));
        }

        Compatibility::Compatible
    }
}

impl PackageMut for Spec<BuildIdent> {
    fn set_build(&mut self, build: Build) {
        self.pkg.set_target(build);
    }
}

impl Recipe for Spec<VersionIdent> {
    type Output = Spec<BuildIdent>;
    type Test = TestSpec;
    type Variant = OptionMap;

    fn ident(&self) -> &VersionIdent {
        &self.pkg
    }

    fn default_variants(&self) -> Cow<'_, Vec<Self::Variant>> {
        Cow::Borrowed(&self.build.variants)
    }

    fn resolve_options(&self, given: &OptionMap) -> Result<OptionMap> {
        let mut resolved = OptionMap::default();
        for opt in self.build.options.iter() {
            let given_value = given.get_for_package(self.pkg.name(), opt.full_name());
            let value = opt.get_value(given_value.map(String::as_ref));
            let compat = opt.validate(Some(&value));
            if !compat.is_ok() {
                return Err(Error::String(compat.to_string()));
            }
            resolved.insert(opt.full_name().to_owned(), value);
        }

        Ok(resolved)
    }

    fn get_build_requirements(&self, options: &OptionMap) -> Result<Cow<'_, RequirementsList>> {
        let build = Build::Digest(options.digest());
        let mut requests = RequirementsList::default();
        for opt in self.build.options.iter() {
            match opt {
                Opt::Pkg(opt) => {
                    let given_value = options.get(opt.pkg.as_opt_name()).map(String::to_owned);
                    let mut req = opt.to_request(
                        given_value,
                        RequestedBy::BinaryBuild(self.ident().to_build(build.clone())),
                    )?;
                    if req.pkg.components.is_empty() {
                        // inject the default component for this context if needed
                        req.pkg.components.insert(Component::default_for_build());
                    }
                    requests.insert_merge(req.into())?;
                }
                Opt::Var(opt) => {
                    // If no value was specified in the spec, there's
                    // no need to turn that into a requirement to
                    // find a var with an empty value.
                    if let Some(value) = options.get(&opt.var) {
                        if !value.is_empty() {
                            requests.insert_merge(opt.to_request(Some(value)).into())?;
                        }
                    }
                }
            }
        }
        Ok(Cow::Owned(requests))
    }

    fn get_tests(&self, stage: TestStage, options: &OptionMap) -> Result<Vec<TestSpec>> {
        let digest = options.digest();
        Ok(self
            .tests
            .iter()
            .filter(|t| t.stage == stage)
            .filter(|t| {
                if t.selectors.is_empty() {
                    return true;
                }
                for selector in t.selectors.iter() {
                    let mut selected_opts = options.clone();
                    selected_opts.extend(selector.clone());
                    if selected_opts.digest() == digest {
                        return true;
                    }
                }
                false
            })
            .cloned()
            .collect())
    }

    fn generate_source_build(&self, root: &Path) -> Result<Spec<BuildIdent>> {
        let mut source = self.clone().map_ident(|i| i.into_build(Build::Source));
        source.prune_for_source_build();
        for source in source.sources.iter_mut() {
            if let SourceSpec::Local(source) = source {
                source.path = root.join(&source.path);
            }
        }
        Ok(source)
    }

    fn generate_binary_build<E>(&self, build_env: E) -> Result<Self::Output>
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest>,
    {
        let mut updated = self.clone();
        let by_name: HashMap<_, _> = build_env
            .members()
            .map(|p| (p.package().name().to_owned(), p))
            .collect();
        let options = build_env.options();
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
                    let spec = by_name.get(&opt.pkg);
                    match spec {
                        None => {
                            return Err(Error::String(format!(
                                "PkgOpt missing in resolved: {}",
                                opt.pkg
                            )));
                        }
                        Some(member) => {
                            let rendered = member
                                .package()
                                .compat()
                                .render(HasVersion::version(member.package()));
                            opt.set_value(rendered)?;
                        }
                    }
                }
            }
        }

        for member in by_name.values() {
            let pkg = member.package();
            let downstream = pkg.downstream_requirements(member.used_components());
            for request in downstream.iter().cloned() {
                let required = Opt::try_from(request.clone())?;
                updated.build.upsert_opt(required.clone());
                updated.install.requirements.insert_merge(request)?;
            }
        }

        updated
            .install
            .render_all_pins(&options, by_name.iter().map(|(_, p)| p.package().ident()))?;
        let digest = updated.resolve_options(&options)?.digest();
        Ok(updated.map_ident(|i| i.into_build(Build::Digest(digest))))
    }
}

impl Satisfy<PkgRequest> for Spec<BuildIdent> {
    fn check_satisfies_request(&self, pkg_request: &PkgRequest) -> Compatibility {
        if pkg_request.pkg.name != *self.pkg.name() {
            return Compatibility::incompatible(format!(
                "different package name: {} != {}",
                pkg_request.pkg.name,
                self.pkg.name()
            ));
        }

        if self.is_deprecated() {
            // deprecated builds are only okay if their build
            // was specifically requested
            if pkg_request.pkg.build.as_ref() != Some(self.pkg.build()) {
                return Compatibility::incompatible(
                    "Build is deprecated and was not specifically requested",
                );
            }
        }

        if pkg_request.prerelease_policy == PreReleasePolicy::ExcludeAll
            && !self.version().pre.is_empty()
        {
            return Compatibility::incompatible("prereleases not allowed");
        }

        let source_package_requested = pkg_request.pkg.build == Some(Build::Source);
        let is_source_build = Package::ident(self).is_source() && !source_package_requested;
        if !pkg_request.pkg.components.is_empty() && !is_source_build {
            let required_components = self
                .components()
                .resolve_uses_names(pkg_request.pkg.components.iter());
            let available_components: BTreeSet<_> = self
                .install
                .components
                .iter()
                .map(|c| c.name.clone())
                .collect();
            let missing_components = required_components
                .difference(&available_components)
                .sorted()
                .collect_vec();
            if !missing_components.is_empty() {
                return Compatibility::incompatible(format!(
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

        let c = pkg_request
            .pkg
            .version
            .is_satisfied_by(self, CompatRule::Binary);
        if !c.is_ok() {
            return c;
        }

        if pkg_request.pkg.build.is_none()
            || pkg_request.pkg.build.as_ref() == Some(self.pkg.build())
        {
            return Compatibility::Compatible;
        }

        Compatibility::incompatible(format!(
            "Package and request differ in builds: requested {:?}, got {:?}",
            pkg_request.pkg.build,
            self.pkg.build()
        ))
    }
}

impl<Ident> Satisfy<VarRequest> for Spec<Ident>
where
    Self: Named,
{
    fn check_satisfies_request(&self, var_request: &VarRequest) -> Compatibility {
        let opt_required = var_request.var.namespace() == Some(self.name());
        let mut opt: Option<&Opt> = None;
        let request_name = &var_request.var;
        for o in self.build.options.iter() {
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
                    return Compatibility::incompatible(format!(
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
                    Compatibility::incompatible(format!(
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

impl<'de> Deserialize<'de> for Spec<VersionIdent>
where
    VersionIdent: serde::de::DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_map(SpecVisitor::recipe())
    }
}

impl<'de> Deserialize<'de> for Spec<AnyIdent>
where
    AnyIdent: serde::de::DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let mut spec = deserializer.deserialize_map(SpecVisitor::default())?;
        if spec.pkg.is_source() {
            // for backward-compatibility with older publishes, prune out anything
            // that is not relevant to a source package, since now source packages
            // can technically have their own requirements, etc.
            spec.prune_for_source_build();
        }
        Ok(spec)
    }
}

impl<'de> Deserialize<'de> for Spec<BuildIdent>
where
    BuildIdent: serde::de::DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let mut spec = deserializer.deserialize_map(SpecVisitor::package())?;
        if spec.pkg.is_source() {
            // for backward-compatibility with older publishes, prune out anything
            // that is not relevant to a source package, since now source packages
            // can technically have their own requirements, etc.
            spec.prune_for_source_build();
        }

        Ok(spec)
    }
}

struct SpecVisitor<B, T> {
    pkg: Option<Ident<B, T>>,
    meta: Option<Meta>,
    compat: Option<Compat>,
    deprecated: Option<bool>,
    sources: Option<Vec<SourceSpec>>,
    build: Option<UncheckedBuildSpec>,
    tests: Option<Vec<TestSpec>>,
    install: Option<InstallSpec>,
    check_build_spec: bool,
}

impl<B, T> Default for SpecVisitor<B, T> {
    fn default() -> Self {
        Self {
            pkg: None,
            meta: None,
            compat: None,
            deprecated: None,
            sources: None,
            build: None,
            tests: None,
            install: None,
            check_build_spec: true,
        }
    }
}

impl SpecVisitor<PkgNameBuf, Version> {
    pub fn recipe() -> Self {
        Self::default()
    }
}

impl SpecVisitor<VersionIdent, Build> {
    // the reassignment here is a simple boolean switch so not a heavy operation
    // or worth having all the extra fields being redefined as None here
    // just like in the default
    #[allow(clippy::field_reassign_with_default)]
    pub fn package() -> Self {
        let mut v = Self::default();
        // if the build is set, we assume that this is a rendered spec and we do
        // not want to make an existing rendered build spec unloadable
        v.check_build_spec = false;
        v
    }
}

impl<'de, B, T> serde::de::Visitor<'de> for SpecVisitor<B, T>
where
    Ident<B, T>: serde::de::DeserializeOwned,
{
    type Value = Spec<Ident<B, T>>;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a package specification")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "pkg" => self.pkg = Some(map.next_value::<Ident<B, T>>()?),
                "meta" => self.meta = Some(map.next_value::<Meta>()?),
                "compat" => self.compat = Some(map.next_value::<Compat>()?),
                "deprecated" => self.deprecated = Some(map.next_value::<bool>()?),
                "sources" => self.sources = Some(map.next_value::<Vec<SourceSpec>>()?),
                "build" => self.build = Some(map.next_value::<UncheckedBuildSpec>()?),
                "tests" => self.tests = Some(map.next_value::<Vec<TestSpec>>()?),
                "install" => self.install = Some(map.next_value::<InstallSpec>()?),
                _ => {
                    // ignore any unrecognized field, but consume the value anyway
                    // TODO: could we warn about fields that look like typos?
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        let pkg = self
            .pkg
            .take()
            .ok_or_else(|| serde::de::Error::missing_field("pkg"))?;
        Ok(Spec {
            meta: self.meta.take().unwrap_or_default(),
            compat: self.compat.take().unwrap_or_default(),
            deprecated: self.deprecated.take().unwrap_or_default(),
            sources: self
                .sources
                .take()
                .unwrap_or_else(|| vec![SourceSpec::Local(LocalSource::default())]),
            build: match self.build.take() {
                Some(build_spec) if !self.check_build_spec => {
                    // Safety: see the SpecVisitor::package constructor
                    unsafe { build_spec.into_inner() }
                }
                Some(build_spec) => build_spec.try_into().map_err(serde::de::Error::custom)?,
                None => Default::default(),
            },
            tests: self.tests.take().unwrap_or_default(),
            install: self.install.take().unwrap_or_default(),
            pkg,
        })
    }
}
