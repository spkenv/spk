// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::convert::TryInto;
use std::path::Path;
use std::str::FromStr;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{
    AsVersionIdent,
    BuildIdent,
    PinnedRequest,
    PinnedValue,
    PkgRequestOptionValue,
};
use spk_schema_foundation::option_map::{OptFilter, Stringified};
use spk_schema_foundation::spec_ops::HasBuildIdent;
use spk_schema_foundation::version::{
    BuildIdProblem,
    CommaSeparated,
    ComponentsMissingProblem,
    IncompatibleReason,
    PackageNameProblem,
    VarOptionProblem,
};

use super::TestSpec;
use crate::build_spec::UncheckedBuildSpec;
use crate::foundation::ident_build::Build;
use crate::foundation::ident_component::Component;
use crate::foundation::name::PkgName;
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, CompatRule, Compatibility, Version};
use crate::foundation::version_range::Ranged;
use crate::ident::{
    PkgRequestWithOptions,
    PreReleasePolicy,
    RequestWithOptions,
    RequestedBy,
    Satisfy,
    VarRequest,
    is_false,
};
use crate::metadata::Meta;
use crate::option::VarOpt;
use crate::package::{BuildOptions, OptionValues};
use crate::spec::SpecTest;
use crate::v0::{EmbeddedPackageSpec, RecipeSpec};
use crate::{
    BuildSpec,
    ComponentSpec,
    ComponentSpecList,
    Components,
    Deprecate,
    DeprecateMut,
    EmbeddedPackagesList,
    EnvOp,
    EnvOpList,
    Inheritance,
    InstallSpec,
    LocalSource,
    Opt,
    Package,
    PackageMut,
    RequirementsList,
    Result,
    RuntimeEnvironment,
    SourceSpec,
    ValidationSpec,
};

#[cfg(test)]
#[path = "./package_spec_test.rs"]
mod package_spec_test;

/// A built package specification.
///
/// This type is used to represent a package that has been built. It is derived
/// from a recipe specification, with options resolved and pinned to the values
/// used in the build.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct PackageSpec {
    pub pkg: BuildIdent,
    #[serde(default, skip_serializing_if = "Meta::is_default")]
    pub meta: Meta,
    #[serde(default, skip_serializing_if = "Compat::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceSpec>,
    // This field is private to update `install_requirements_with_options`
    // when it is modified.
    #[serde(default, skip_serializing_if = "BuildSpec::is_default")]
    build: BuildSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<TestSpec>,
    // This field is private to update `install_requirements_with_options`
    // when it is modified.
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    install: InstallSpec<PinnedRequest>,
    /// Install requirements with options included.
    ///
    /// This value is not serialized; it is populated when loading or when build
    /// or install are modified.
    #[serde(skip)]
    install_requirements_with_options: RequirementsList<RequestWithOptions>,
}

impl PackageSpec {
    /// Create an empty spec for the identified package
    pub fn new(ident: BuildIdent) -> Self {
        Self {
            pkg: ident,
            meta: Meta::default(),
            compat: Compat::default(),
            deprecated: bool::default(),
            sources: Vec::new(),
            build: BuildSpec::default(),
            tests: Vec::new(),
            install: InstallSpec::default(),
            install_requirements_with_options: RequirementsList::default(),
        }
    }

    fn calculate_install_requirements_with_options(
        build: &BuildSpec,
        install: &InstallSpec<PinnedRequest>,
    ) -> RequirementsList<RequestWithOptions> {
        (build.options.iter(), &install.requirements).into()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_from_parts(
        pkg: BuildIdent,
        meta: Meta,
        compat: Compat,
        deprecated: bool,
        sources: Vec<SourceSpec>,
        build: BuildSpec,
        tests: Vec<TestSpec>,
        install: InstallSpec<PinnedRequest>,
    ) -> Self {
        let install_requirements_with_options =
            Self::calculate_install_requirements_with_options(&build, &install);

        Self {
            pkg,
            meta,
            compat,
            deprecated,
            sources,
            build,
            tests,
            install,
            install_requirements_with_options,
        }
    }

    /// Create a source package from the given recipe.
    ///
    /// The paths to the sources will be rooted at the given `root` path.
    pub fn new_source_package_from_recipe_with_root(recipe: RecipeSpec, root: &Path) -> Self {
        let mut spec = Self::new_from_parts(
            recipe.pkg.into_build_ident(Build::Source),
            recipe.meta,
            recipe.compat,
            recipe.deprecated,
            recipe.sources,
            recipe.build.into(),
            recipe.tests,
            InstallSpec::default(),
        );
        spec.prune_for_source_build();
        for source in spec.sources.iter_mut() {
            if let SourceSpec::Local(source) = source {
                source.path = root.join(&source.path);
            }
        }
        spec
    }

    /// Read-only access to the build spec
    #[inline]
    pub fn build(&self) -> &BuildSpec {
        &self.build
    }

    /// Read-write access to the build spec
    pub fn build_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut BuildSpec) -> R,
    {
        let r = f(&mut self.build);
        self.install_requirements_with_options =
            Self::calculate_install_requirements_with_options(&self.build, &self.install);
        r
    }

    /// Remove requirements and other package data that
    /// is not relevant for a source package build.
    pub(super) fn prune_for_source_build(&mut self) {
        self.install.requirements.clear();
        self.build = Default::default();
        self.tests.clear();
        self.install.components.clear();
        self.install.components.push(
            // This originally used FileMatcher::default() but now via this
            // call it uses FileMatcher::all(). Is there any difference?
            ComponentSpec::default_source(),
        );
    }

    /// Read-only access to the install spec
    #[inline]
    pub fn install(&self) -> &InstallSpec<PinnedRequest> {
        &self.install
    }

    /// Read-write access to the install spec
    pub fn install_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut InstallSpec<PinnedRequest>) -> R,
    {
        let r = f(&mut self.install);
        self.install_requirements_with_options =
            Self::calculate_install_requirements_with_options(&self.build, &self.install);
        r
    }

    /// Update install.environment using the provided build environment
    /// variables.
    pub fn update_install_environment_with_env(
        &mut self,
        mut build_env_vars: HashMap<String, String>,
    ) {
        let mut updated_ops = EnvOpList::default();
        build_env_vars.extend(self.get_build_env());
        for op in self.install.environment.iter() {
            updated_ops.push(op.to_expanded(&build_env_vars));
        }
        // This modification of self.install does not require updating
        // install_requirements_with_options
        self.install.environment = updated_ops;
    }
}

impl PackageSpec {
    /// Return downstream var requirements that match the given filter.
    fn downstream_requirements<F>(&self, filter: F) -> Cow<'_, RequirementsList<RequestWithOptions>>
    where
        F: FnMut(&&VarOpt) -> bool,
    {
        let requests = self
            .build
            .options
            .iter()
            .filter_map(|opt| match opt {
                Opt::Var(v) => Some(v),
                Opt::Pkg(_) => None,
            })
            .filter(filter)
            .map(|o| {
                let var = o.var.with_default_namespace(self.name());
                VarRequest {
                    var,
                    // we are assuming that the var here will have a value because
                    // this is a built binary package
                    value: o.get_value(None).unwrap_or_default().into(),
                    description: o.description.clone(),
                }
            })
            .map(RequestWithOptions::Var);
        RequirementsList::<RequestWithOptions>::try_from_iter(requests)
            .map(Cow::Owned)
            .expect("build opts do not contain duplicates")
    }
}

impl BuildOptions for PackageSpec {
    fn build_options(&self) -> Cow<'_, [Opt]> {
        Cow::Borrowed(&self.build.options)
    }
}

impl Components for PackageSpec {
    type ComponentSpecT = ComponentSpec;

    fn components(&self) -> &ComponentSpecList<Self::ComponentSpecT> {
        &self.install.components
    }
}

impl Deprecate for PackageSpec {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl DeprecateMut for PackageSpec {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl HasBuild for PackageSpec {
    fn build(&self) -> &Build {
        self.pkg.build()
    }
}

impl HasBuildIdent for PackageSpec {
    fn build_ident(&self) -> &BuildIdent {
        &self.pkg
    }
}

impl HasVersion for PackageSpec {
    fn version(&self) -> &Version {
        self.pkg.version()
    }
}

impl Named for PackageSpec {
    fn name(&self) -> &PkgName {
        self.pkg.name()
    }
}

impl RuntimeEnvironment for PackageSpec {
    fn runtime_environment(&self) -> &[EnvOp] {
        &self.install.environment
    }
}

impl Versioned for PackageSpec {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl OptionValues for PackageSpec {
    fn option_values(&self) -> OptionMap {
        let mut opts = OptionMap::default();
        for opt in self.build.options.iter() {
            // since this is a PackageSpec we can assume that this spec has
            // had all of the options pinned/resolved.
            opts.insert(opt.full_name().to_owned(), opt.get_value(None));
        }
        opts
    }
}

impl Package for PackageSpec {
    type Package = Self;
    type EmbeddedPackage = EmbeddedPackageSpec;

    fn ident(&self) -> &BuildIdent {
        &self.pkg
    }

    fn metadata(&self) -> &crate::metadata::Meta {
        &self.meta
    }

    fn matches_all_filters(&self, filter_by: &Option<Vec<OptFilter>>) -> bool {
        if let Some(filters) = filter_by {
            let settings = self.option_values();

            for filter in filters {
                if !settings.contains_key(&filter.name) {
                    // Not having an option with the filter's name is
                    // considered a match.
                    continue;
                }

                let var_request =
                    VarRequest::new_with_value(filter.name.clone(), filter.value.clone());

                let compat = self.check_satisfies_request(&var_request);
                if !compat.is_ok() {
                    return false;
                }
            }
        }
        // All the filters match, or there were no filters
        true
    }

    fn sources(&self) -> &Vec<SourceSpec> {
        &self.sources
    }

    fn embedded(&self) -> &EmbeddedPackagesList<Self::EmbeddedPackage> {
        &self.install.embedded
    }

    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<Vec<(Self::Package, Option<Component>)>, &str> {
        Ok(self
            .install
            .embedded
            .iter()
            .map(|embed| (embed.clone().into(), None))
            .collect())
    }

    fn get_build_options(&self) -> &Vec<Opt> {
        &self.build.options
    }

    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList<PinnedRequest>>> {
        let mut requests = RequirementsList::default();
        for opt in self.build.options.iter() {
            match opt {
                Opt::Pkg(opt) => {
                    let mut req =
                        opt.to_request(None, RequestedBy::BinaryBuild(self.ident().clone()))?;
                    if req.pkg.components.is_empty() {
                        // inject the default component for this context if needed
                        req.pkg.components.insert(Component::default_for_build());
                    }
                    requests.insert_or_merge_pinned(PinnedRequest::Pkg(req))?;
                }
                Opt::Var(opt) => {
                    // If no value was specified in the spec, there's
                    // no need to turn that into a requirement to
                    // find a var with an empty value.
                    if let Some(value) = opt.get_value(None)
                        && !value.is_empty()
                    {
                        requests.insert_or_merge_pinned(PinnedRequest::Var(
                            opt.to_request(Some(value.as_str())),
                        ))?;
                    }
                }
            }
        }
        Ok(Cow::Owned(requests))
    }

    fn runtime_requirements(&self) -> Cow<'_, crate::RequirementsList<RequestWithOptions>> {
        Cow::Borrowed(&self.install_requirements_with_options)
    }

    fn get_all_tests(&self) -> Vec<SpecTest> {
        self.tests.clone().into_iter().map(SpecTest::V0).collect()
    }

    fn downstream_build_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList<RequestWithOptions>> {
        self.downstream_requirements(|o| o.inheritance() != Inheritance::Weak)
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList<RequestWithOptions>> {
        self.downstream_requirements(|o| o.inheritance() == Inheritance::Strong)
    }

    fn validation(&self) -> &ValidationSpec {
        &self.build.validation
    }

    fn build_script(&self) -> String {
        self.build.script.join("\n")
    }
}

impl PackageMut for PackageSpec {
    fn set_build(&mut self, build: Build) {
        self.pkg.set_target(build);
    }
}

/// Shared implementation for Satisfy<PkgRequestWithOptions> for package-like types.
pub(crate) fn check_package_spec_satisfies_pkg_request<T>(
    spec: &T,
    pkg_request_with_options: &PkgRequestWithOptions,
) -> Compatibility
where
    T: BuildOptions + Components + Deprecate + HasBuild + Named + Versioned,
    <T as Components>::ComponentSpecT: ComponentOps,
{
    let PkgRequestWithOptions {
        pkg_request,
        options,
    } = pkg_request_with_options;

    if pkg_request.pkg.name != *spec.name() {
        return Compatibility::Incompatible(IncompatibleReason::PackageNameMismatch(
            PackageNameProblem::PkgRequest {
                self_name: spec.name().to_owned(),
                other_name: pkg_request.pkg.name.clone(),
            },
        ));
    }

    if spec.is_deprecated() {
        // deprecated builds are only okay if their build
        // was specifically requested
        if pkg_request.pkg.build.as_ref() != Some(spec.build()) {
            return Compatibility::Incompatible(IncompatibleReason::BuildDeprecated);
        }
    }

    if (pkg_request.prerelease_policy.is_none()
        || pkg_request.prerelease_policy == Some(PreReleasePolicy::ExcludeAll))
        && !spec.version().pre.is_empty()
    {
        return Compatibility::Incompatible(IncompatibleReason::PrereleasesNotAllowed);
    }

    let source_package_requested = pkg_request.pkg.build == Some(Build::Source);
    let is_source_build = spec.build().is_source() && !source_package_requested;
    if !pkg_request.pkg.components.is_empty() && !is_source_build {
        let required_components = spec
            .components()
            .resolve_uses(pkg_request.pkg.components.iter());
        let available_components: BTreeSet<_> =
            spec.components().iter().map(|c| c.name().clone()).collect();
        let missing_components = required_components
            .difference(&available_components)
            .sorted()
            .collect_vec();
        if !missing_components.is_empty() {
            return Compatibility::Incompatible(IncompatibleReason::ComponentsMissing(
                ComponentsMissingProblem::ComponentsNotDefined {
                    missing: CommaSeparated(
                        missing_components
                            .into_iter()
                            .map(Component::to_string)
                            .collect(),
                    ),
                    available: CommaSeparated(
                        available_components
                            .into_iter()
                            .map(|c| c.to_string())
                            .collect(),
                    ),
                },
            ));
        }
    }

    let c = pkg_request
        .pkg
        .version
        .is_satisfied_by(spec, CompatRule::Binary);
    if !c.is_ok() {
        return c;
    }

    if !(pkg_request.pkg.build.is_none() || pkg_request.pkg.build.as_ref() == Some(spec.build())) {
        return Compatibility::Incompatible(IncompatibleReason::BuildIdMismatch(
            BuildIdProblem::PkgRequest {
                self_build: spec.build().clone(),
                requested: pkg_request.pkg.build.clone(),
            },
        ));
    }

    // Any required options must be included in the request's options.
    for opt in spec.build_options().iter() {
        let Opt::Var(opt) = opt else {
            continue;
        };
        if !opt.required {
            continue;
        }
        // XXX: Have to construct this namespaced string at runtime, should
        // `options` be restructured instead? It could categorize options
        // by namespace.
        let namespaced_var = opt.var.with_namespace(spec.name());
        // All the merged requests must have requested the option for the
        // request to satisfy the "required" property.
        let Some(PkgRequestOptionValue::Complete(value)) = options.get(&namespaced_var) else {
            return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                VarOptionProblem::RequiredButMissing {
                    opt_name: namespaced_var,
                },
            ));
        };
        if let Some(expected) = opt.get_value(None)
            && *value != expected
        {
            return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                VarOptionProblem::IncompatibleBuildOption {
                    var_request: opt.var.clone(),
                    exact: expected,
                    request_value: value.to_string(),
                },
            ));
        }
    }

    Compatibility::Compatible
}

impl Satisfy<PkgRequestWithOptions> for PackageSpec {
    fn check_satisfies_request(&self, pkg_request: &PkgRequestWithOptions) -> Compatibility {
        check_package_spec_satisfies_pkg_request(self, pkg_request)
    }
}

impl Satisfy<VarRequest<PinnedValue>> for PackageSpec
where
    Self: Named,
{
    fn check_satisfies_request(&self, var_request: &VarRequest<PinnedValue>) -> Compatibility {
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
                    return Compatibility::Incompatible(IncompatibleReason::VarOptionMissing(
                        var_request.var.clone(),
                    ));
                }
                Compatibility::Compatible
            }
            Some(Opt::Pkg(opt)) => opt.validate(Some(&*var_request.value)),
            Some(Opt::Var(opt)) => {
                let request_value = &*var_request.value;
                let exact = opt.get_value(Some(request_value));
                if exact.as_deref() == Some(request_value) {
                    return Compatibility::Compatible;
                }

                // For values that aren't exact matches, if the option specifies
                // a compat rule, try treating the values as version numbers
                // and see if they satisfy the rule.
                if let Some(compat) = &opt.compat {
                    let base_version = exact.clone();
                    let Ok(base_version) = Version::from_str(&base_version.unwrap_or_default())
                    else {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionInvalidVersion {
                                var_request: var_request.var.clone(),
                                base: exact.unwrap_or_default(),
                                request_value: request_value.to_string(),
                            },
                        ));
                    };

                    let Ok(request_version) = Version::from_str(request_value) else {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionInvalidVersion {
                                var_request: var_request.var.clone(),
                                base: exact.unwrap_or_default(),
                                request_value: request_value.to_string(),
                            },
                        ));
                    };

                    let result = compat.is_binary_compatible(&base_version, &request_version);
                    if let Compatibility::Incompatible(incompatible) = result {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionWithContext {
                                var_request: var_request.var.clone(),
                                exact: exact.unwrap_or_else(|| "None".to_string()),
                                request_value: request_value.to_string(),
                                context: Box::new(incompatible),
                            },
                        ));
                    }
                    return result;
                }

                Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                    VarOptionProblem::IncompatibleBuildOption {
                        var_request: var_request.var.clone(),
                        exact: exact.unwrap_or_else(|| "None".to_string()),
                        request_value: request_value.to_string(),
                    },
                ))
            }
        }
    }
}

impl From<EmbeddedPackageSpec> for PackageSpec {
    fn from(embed: EmbeddedPackageSpec) -> Self {
        Self {
            build: embed.build().clone().into(),
            install: embed.install().clone().into(),
            install_requirements_with_options: embed.install_requirements_with_options().clone(),
            pkg: embed.pkg,
            meta: embed.meta,
            compat: embed.compat,
            deprecated: embed.deprecated,
            sources: embed.sources,
            tests: embed.tests,
        }
    }
}

impl<'de> Deserialize<'de> for PackageSpec {
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

struct SpecVisitor {
    pkg: Option<BuildIdent>,
    meta: Option<Meta>,
    compat: Option<Compat>,
    deprecated: Option<bool>,
    sources: Option<Vec<SourceSpec>>,
    build: Option<UncheckedBuildSpec>,
    tests: Option<Vec<TestSpec>>,
    install: Option<InstallSpec<PinnedRequest>>,
    check_build_spec: bool,
}

impl SpecVisitor {
    #[inline]
    pub fn with_check_build_spec(check_build_spec: bool) -> Self {
        Self {
            pkg: None,
            meta: None,
            compat: None,
            deprecated: None,
            sources: None,
            build: None,
            tests: None,
            install: None,
            check_build_spec,
        }
    }
}

impl Default for SpecVisitor {
    #[inline]
    fn default() -> Self {
        Self::with_check_build_spec(true)
    }
}

impl SpecVisitor {
    #[allow(clippy::field_reassign_with_default)]
    pub fn package() -> Self {
        // if the build is set, we assume that this is a rendered spec and we do
        // not want to make an existing rendered build spec unloadable
        Self::with_check_build_spec(false)
    }
}

impl<'de> serde::de::Visitor<'de> for SpecVisitor {
    type Value = PackageSpec;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a package specification")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "pkg" => self.pkg = Some(map.next_value::<BuildIdent>()?),
                "meta" => self.meta = Some(map.next_value::<Meta>()?),
                "compat" => self.compat = Some(map.next_value::<Compat>()?),
                "deprecated" => self.deprecated = Some(map.next_value::<bool>()?),
                "sources" => self.sources = Some(map.next_value::<Vec<SourceSpec>>()?),
                "build" => self.build = Some(map.next_value::<UncheckedBuildSpec>()?),
                "tests" => self.tests = Some(map.next_value::<Vec<TestSpec>>()?),
                "install" => self.install = Some(map.next_value::<InstallSpec<PinnedRequest>>()?),
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

        // Update the requester field of any test requirements.
        let mut tests = self.tests.take().unwrap_or_default();
        for test in tests.iter_mut() {
            test.add_requester(pkg.as_version_ident());
        }

        Ok(PackageSpec::new_from_parts(
            pkg,
            self.meta.take().unwrap_or_default(),
            self.compat.take().unwrap_or_default(),
            self.deprecated.take().unwrap_or_default(),
            self.sources
                .take()
                .unwrap_or_else(|| vec![SourceSpec::Local(LocalSource::default())]),
            match self.build.take() {
                Some(build_spec) if !self.check_build_spec => {
                    // Safety: see the SpecVisitor::package constructor
                    unsafe { build_spec.into_inner() }
                }
                Some(build_spec) => build_spec.try_into().map_err(serde::de::Error::custom)?,
                None => Default::default(),
            },
            tests,
            self.install.take().unwrap_or_default(),
        ))
    }
}
