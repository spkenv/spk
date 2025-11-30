// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{AsVersionIdent, RangeIdent, VersionIdent};
use spk_schema_foundation::ident_build::BuildId;
use spk_schema_foundation::ident_component::ComponentBTreeSet;
use spk_schema_foundation::option_map::Stringified;
use spk_schema_foundation::version::{IncompatibleReason, VarOptionProblem};
use variantly::Variantly;

use super::TestSpec;
use super::variant_spec::VariantSpecEntryKey;
use crate::build_spec::UncheckedBuildSpec;
use crate::foundation::ident_build::Build;
use crate::foundation::ident_component::Component;
use crate::foundation::name::{OptNameBuf, PkgName};
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{PkgRequest, Request, RequestedBy, Satisfy, VarRequest, is_false};
use crate::metadata::Meta;
use crate::option::VarOpt;
use crate::v0::{PackageSpec, RecipeInstallSpec};
use crate::{
    BuildEnv,
    BuildSpec,
    Deprecate,
    DeprecateMut,
    EnvOp,
    EnvOpList,
    Error,
    InputVariant,
    LocalSource,
    Opt,
    Package,
    Recipe,
    RequirementsList,
    Result,
    RuntimeEnvironment,
    SourceSpec,
    TestStage,
    Variant,
};

#[cfg(test)]
#[path = "./recipe_spec_test.rs"]
mod recipe_spec_test;

/// A package recipe specification.
///
/// This roughly maps to the expected contents of a package recipe that a user
/// creates, but after any template processing has been applied.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct RecipeSpec {
    pub pkg: VersionIdent,
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
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    pub install: RecipeInstallSpec,
}

impl RecipeSpec {
    /// Create an empty spec for the identified package
    pub fn new(ident: VersionIdent) -> Self {
        Self {
            pkg: ident,
            meta: Meta::default(),
            compat: Compat::default(),
            deprecated: bool::default(),
            sources: Vec::new(),
            build: BuildSpec::default(),
            tests: Vec::new(),
            install: RecipeInstallSpec::default(),
        }
    }

    pub fn build_options(&self) -> Cow<'_, [Opt]> {
        Cow::Borrowed(self.build.options.as_slice())
    }
}

impl Deprecate for RecipeSpec {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl DeprecateMut for RecipeSpec {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl HasVersion for RecipeSpec {
    fn version(&self) -> &Version {
        self.pkg.version()
    }
}

impl Named for RecipeSpec {
    fn name(&self) -> &PkgName {
        self.pkg.name()
    }
}

impl RuntimeEnvironment for RecipeSpec {
    fn runtime_environment(&self) -> &[EnvOp] {
        &self.install.environment
    }
}

impl Versioned for RecipeSpec {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl Recipe for RecipeSpec {
    type Output = PackageSpec;
    type Variant = super::Variant;
    type Test = TestSpec;

    fn ident(&self) -> &VersionIdent {
        &self.pkg
    }

    #[inline]
    fn build_digest<V>(&self, variant: &V) -> Result<BuildId>
    where
        V: Variant,
    {
        self.build.build_digest(self.pkg.name(), variant)
    }

    fn default_variants(&self, options: &OptionMap) -> Cow<'_, Vec<Self::Variant>> {
        if self.build.variants.is_empty() {
            Cow::Owned(vec![super::Variant::from_build_options(
                &self.build.options,
                options,
            )])
        } else {
            Cow::Borrowed(&self.build.variants)
        }
    }

    fn resolve_options<V>(&self, variant: &V) -> Result<OptionMap>
    where
        V: Variant,
    {
        self.build
            .resolve_options_for_pkg_name(self.name(), variant)
            .map(|(options, _)| options)
    }

    fn get_build_requirements<V>(&self, variant: &V) -> Result<Cow<'_, RequirementsList>>
    where
        V: Variant,
    {
        let opts = self.build.opts_for_variant(variant)?;
        let options = self.resolve_options(variant)?;
        let build_digest = Build::BuildId(self.build_digest(variant)?);
        let mut requests = RequirementsList::default();
        for opt in opts {
            match opt {
                Opt::Pkg(opt) => {
                    let given_value = options.get(opt.pkg.as_opt_name()).map(String::to_owned);
                    let mut req = opt.to_request(
                        given_value,
                        RequestedBy::BinaryBuild(self.ident().to_build_ident(build_digest.clone())),
                    )?;
                    if req.pkg.components.is_empty() {
                        // inject the default component for this context if needed
                        req.pkg.components.insert(Component::default_for_build());
                    }
                    requests.insert_or_merge(req.into())?;
                }
                Opt::Var(opt) => {
                    // If no value was specified in the spec, there's
                    // no need to turn that into a requirement to
                    // find a var with an empty value.
                    if let Some(value) = options.get(&opt.var)
                        && !value.is_empty()
                    {
                        requests.insert_or_merge(opt.to_request(Some(value)).into())?;
                    }
                }
            }
        }
        Ok(Cow::Owned(requests))
    }

    fn get_tests<V>(&self, stage: TestStage, variant: &V) -> Result<Vec<TestSpec>>
    where
        V: Variant,
    {
        let options = self.resolve_options(variant)?;
        Ok(self
            .tests
            .iter()
            .filter(|t| t.stage == stage)
            .filter(|t| {
                if t.selectors.is_empty() {
                    return true;
                }
                for selector in t.selectors.iter() {
                    // We need to check if this selector matches the variant.
                    // It isn't required to specify everything from the variant,
                    // just everything specified in the selector has to match
                    // what is in the variant.
                    for (key, value) in &selector.entries {
                        match key {
                            VariantSpecEntryKey::PkgOrOpt(pkg) => {
                                // First the version asked for must match.
                                if options.get(pkg.0.name.as_opt_name()) != Some(value) {
                                    return false;
                                }
                                // Then the components asked for must be a
                                // subset of what is present.
                                if !self
                                    .get_build_requirements(variant)
                                    .unwrap_or_default()
                                    .iter()
                                    .any(|req| match req {
                                        Request::Pkg(PkgRequest {
                                            pkg:
                                                RangeIdent {
                                                    name, components, ..
                                                },
                                            ..
                                        }) => {
                                            *name == pkg.0.name
                                                && ComponentBTreeSet::new(components).satisfies(
                                                    &ComponentBTreeSet::new(&pkg.0.components),
                                                )
                                        }
                                        Request::Var(VarRequest {
                                            var,
                                            value: var_request_value,
                                            ..
                                        }) => {
                                            // The variant spec entry may be
                                            // an "opt" like "distro" but this
                                            // is only possible if there were
                                            // no components specified.
                                            pkg.0.components.is_empty()
                                                && var.as_str() == pkg.0.name.as_str()
                                                && var_request_value.as_pinned()
                                                    == Some(value.as_str())
                                        }
                                    })
                                {
                                    return false;
                                }
                            }
                            VariantSpecEntryKey::Opt(opt) => {
                                if options.get(opt) != Some(value) {
                                    return false;
                                }
                            }
                        }
                    }
                }
                true
            })
            .cloned()
            .collect())
    }

    fn generate_source_build(&self, root: &Path) -> Result<PackageSpec> {
        let mut source = PackageSpec {
            pkg: self.pkg.clone().into_build_ident(Build::Source),
            meta: self.meta.clone(),
            compat: self.compat.clone(),
            deprecated: self.deprecated,
            build: self.build.clone(),
            install: self.install.clone().into(),
            sources: self.sources.clone(),
            tests: self.tests.clone(),
        };
        source.prune_for_source_build();
        for source in source.sources.iter_mut() {
            if let SourceSpec::Local(source) = source {
                source.path = root.join(&source.path);
            }
        }
        Ok(source)
    }

    fn generate_binary_build<V, E, P>(self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package,
    {
        let build_requirements = self.get_build_requirements(variant)?.into_owned();

        let build_options = variant.options();
        let original_build = self.build.clone();
        let mut updated = self;
        updated.build.options = updated.build.opts_for_variant(variant)?;

        let specs: HashMap<_, _> = build_env
            .build_env()
            .into_iter()
            .map(|p| (p.name().to_owned(), p))
            .collect();

        #[derive(Clone, Copy, Variantly)]
        enum RequirePkgInBuildEnv {
            Yes,
            No,
        }

        let pin_options = |options: &mut [Opt], require_pkg_in_build_env: RequirePkgInBuildEnv| {
            for opt in options.iter_mut() {
                match opt {
                    Opt::Var(opt) => {
                        opt.set_value(
                            build_options
                                .get(&opt.var)
                                .or_else(|| build_options.get(opt.var.without_namespace()))
                                .map(String::to_owned)
                                .or_else(|| opt.get_value(None))
                                .unwrap_or_default(),
                        )?;
                    }
                    Opt::Pkg(opt) => {
                        let spec = specs.get(&opt.pkg);
                        match spec {
                            None if require_pkg_in_build_env.is_yes() => {
                                return Err(Error::String(format!(
                                    "PkgOpt missing in resolved: {}",
                                    opt.pkg
                                )));
                            }
                            None => {}
                            Some(spec) => {
                                let rendered = spec.compat().render(HasVersion::version(spec));
                                opt.set_value(rendered)?;
                            }
                        }
                    }
                }
            }
            Ok(())
        };

        pin_options(&mut updated.build.options, RequirePkgInBuildEnv::Yes)?;
        for embedded in updated.install.embedded.iter_mut() {
            pin_options(
                &mut embedded.build.options,
                // An embedded package that says it depends on a package
                // "foo" that the parent package doesn't have in its build
                // requirements means that "foo" will not necessarily be in
                // the build env. That shouldn't prevent the parent package
                // from building, but also the embedded stub will not get
                // its build requirement for "foo" pinned. It is impossible
                // to "build" an embedded package anyway; build pkg
                // requirements in an embedded package are basically
                // meaningless.
                RequirePkgInBuildEnv::No,
            )?;
        }

        updated
            .install
            .render_all_pins(&build_options, specs.values().map(|p| p.ident()))?;

        // Update metadata fields from the output of the executable.
        let config = match spk_config::get_config() {
            Ok(c) => c,
            Err(err) => return Err(Error::String(format!("Failed to load spk config: {err}"))),
        };

        if let Err(err) = updated.meta.update_metadata(&config.metadata) {
            tracing::warn!("Failed to collect extra package metadata: {err}");
        }

        let mut missing_build_requirements = HashMap::new();
        let mut missing_runtime_requirements: HashMap<OptNameBuf, (String, Option<String>)> =
            HashMap::new();

        for (_, spec) in specs {
            let downstream_build = spec.downstream_build_requirements([]);
            for request in downstream_build.iter() {
                match build_requirements.contains_request(request) {
                    Compatibility::Compatible => continue,
                    Compatibility::Incompatible(_) => match request {
                        Request::Pkg(_) => continue,
                        Request::Var(var) => {
                            let Some(value) = var.value.as_pinned() else {
                                continue;
                            };
                            match missing_build_requirements.entry(var.var.clone()) {
                                std::collections::hash_map::Entry::Occupied(entry) => {
                                    if entry.get() != value {
                                        return Err(Error::String(format!(
                                            "Multiple conflicting downstream build requirements found for {}: {} and {}",
                                            var.var,
                                            entry.get(),
                                            value
                                        )));
                                    }
                                }
                                std::collections::hash_map::Entry::Vacant(vacant) => {
                                    vacant.insert(value.to_string());
                                }
                            }
                        }
                    },
                }
            }
            let downstream_runtime = spec.downstream_runtime_requirements([]);
            for request in downstream_runtime.iter() {
                match updated.install.requirements.contains_request(request) {
                    Compatibility::Compatible => continue,
                    Compatibility::Incompatible(_) => match request {
                        Request::Pkg(_) => continue,
                        Request::Var(var) => {
                            let Some(value) = var.value.as_pinned() else {
                                continue;
                            };
                            match missing_runtime_requirements.entry(var.var.clone()) {
                                std::collections::hash_map::Entry::Occupied(entry) => {
                                    if entry.get().0 != value {
                                        return Err(Error::String(format!(
                                            "Multiple conflicting downstream runtime requirements found for {}: {} and {}",
                                            var.var,
                                            &entry.get().0,
                                            value
                                        )));
                                    }
                                }
                                std::collections::hash_map::Entry::Vacant(vacant) => {
                                    vacant.insert((value.to_string(), var.description.clone()));
                                }
                            }
                        }
                    },
                }
            }
        }

        for req in missing_build_requirements {
            let mut var = VarOpt::new(req.0)?;
            var.set_value(req.1)?;
            updated.build.options.push(Opt::Var(var));
        }
        for (name, (value, description)) in missing_runtime_requirements {
            updated.install.requirements.insert_or_merge(Request::Var(
                VarRequest::new_with_description(name, value, description.as_ref()),
            ))?;
        }

        // Calculate the digest from the non-updated spec so it isn't affected
        // by `build_env`. The digest is expected to be based solely on the
        // input options and recipe.
        let digest = original_build.build_digest(updated.pkg.name(), variant.input_variant())?;
        let mut build = PackageSpec {
            pkg: updated.pkg.into_build_ident(Build::BuildId(digest)),
            meta: updated.meta,
            compat: updated.compat,
            deprecated: updated.deprecated,
            build: updated.build,
            install: updated.install.into(),
            sources: updated.sources,
            tests: updated.tests,
        };

        // Expand env variables from EnvOp.
        let mut updated_ops = EnvOpList::default();
        let mut build_env_vars = build_env.env_vars();
        build_env_vars.extend(build.get_build_env());
        for op in build.install.environment.iter() {
            updated_ops.push(op.to_expanded(&build_env_vars));
        }
        build.install.environment = updated_ops;

        Ok(build)
    }

    fn metadata(&self) -> &Meta {
        &self.meta
    }
}

impl Satisfy<VarRequest> for RecipeSpec
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
                    return Compatibility::Incompatible(IncompatibleReason::VarOptionMissing(
                        var_request.var.clone(),
                    ));
                }
                Compatibility::Compatible
            }
            Some(Opt::Pkg(opt)) => opt.validate(var_request.value.as_pinned()),
            Some(Opt::Var(opt)) => {
                let request_value = var_request.value.as_pinned();
                let exact = opt.get_value(request_value);
                if exact.as_deref() == request_value {
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
                                request_value: request_value.unwrap_or_default().to_string(),
                            },
                        ));
                    };

                    let Ok(request_version) = Version::from_str(request_value.unwrap_or_default())
                    else {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionInvalidVersion {
                                var_request: var_request.var.clone(),
                                base: exact.unwrap_or_default(),
                                request_value: request_value.unwrap_or_default().to_string(),
                            },
                        ));
                    };

                    let result = compat.is_binary_compatible(&base_version, &request_version);
                    if let Compatibility::Incompatible(incompatible) = result {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionWithContext {
                                var_request: var_request.var.clone(),
                                exact: exact.unwrap_or_else(|| "None".to_string()),
                                request_value: request_value.unwrap_or_default().to_string(),
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
                        request_value: request_value.unwrap_or_default().to_string(),
                    },
                ))
            }
        }
    }
}

impl<'de> Deserialize<'de> for RecipeSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_map(SpecVisitor::recipe())
    }
}

struct SpecVisitor {
    pkg: Option<VersionIdent>,
    meta: Option<Meta>,
    compat: Option<Compat>,
    deprecated: Option<bool>,
    sources: Option<Vec<SourceSpec>>,
    build: Option<UncheckedBuildSpec>,
    tests: Option<Vec<TestSpec>>,
    install: Option<RecipeInstallSpec>,
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
    pub fn recipe() -> Self {
        Self::default()
    }
}

impl<'de> serde::de::Visitor<'de> for SpecVisitor {
    type Value = RecipeSpec;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a package recipe specification")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "pkg" => self.pkg = Some(map.next_value::<VersionIdent>()?),
                "meta" => self.meta = Some(map.next_value::<Meta>()?),
                "compat" => self.compat = Some(map.next_value::<Compat>()?),
                "deprecated" => self.deprecated = Some(map.next_value::<bool>()?),
                "sources" => self.sources = Some(map.next_value::<Vec<SourceSpec>>()?),
                "build" => self.build = Some(map.next_value::<UncheckedBuildSpec>()?),
                "tests" => self.tests = Some(map.next_value::<Vec<TestSpec>>()?),
                "install" => self.install = Some(map.next_value::<RecipeInstallSpec>()?),
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

        Ok(RecipeSpec {
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
            tests,
            install: self.install.take().unwrap_or_default(),
            pkg,
        })
    }
}

/// Convert from a PackageSpec to a RecipeSpec.
///
/// This conversion direction does not make sense in general but tests make use
/// of `make_repo!` to create source packages, and a recipe must be derived from
/// the package spec. The lossy nature of this conversion is acceptable for
/// tests.
impl From<PackageSpec> for RecipeSpec {
    fn from(pkg: PackageSpec) -> Self {
        Self {
            pkg: pkg.pkg.as_version_ident().clone(),
            meta: pkg.meta,
            compat: pkg.compat,
            deprecated: pkg.deprecated,
            sources: pkg.sources,
            build: pkg.build,
            tests: pkg.tests,
            install: pkg.install.into(),
        }
    }
}
