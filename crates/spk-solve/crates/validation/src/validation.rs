// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::collections::{HashMap, HashSet, HashSet};
use std::mem::take;
use std::sync::Arc;

use enum_dispatch::enum_dispatch;
use itertools::Itertools;
use spk_schema::foundation::format::{FormatChangeOptions, FormatRequest};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::PkgNameBuf;
use spk_schema::foundation::spec_ops::Named;
use spk_schema::foundation::version::Compatibility;
use spk_schema::ident::{PkgRequest, Request, Satisfy, VarRequest};
use spk_schema::ident_build::{Build, EmbeddedSource};
use spk_schema::{Package, Recipe, Spec};
use spk_solve_graph::{CachedHash, GetMergedRequestError, State};
use spk_solve_solution::PackageSource;
use spk_storage::RepositoryHandle;

use crate::{Error, Result};

#[cfg(test)]
#[path = "./validation_test.rs"]
mod validation_test;

/// A tracing target for the impossible request checks
pub const IMPOSSIBLE_REQUEST_TARGET: &str = "impossible_requests";

#[derive(Clone, Copy)]
#[enum_dispatch(ValidatorT)]
pub enum Validators {
    Deprecation(DeprecationValidator),
    BinaryOnly(BinaryOnlyValidator),
    PackageRequest(PkgRequestValidator),
    Components(ComponentsValidator),
    Options(OptionsValidator),
    VarRequirements(VarRequirementsValidator),
    PkgRequirements(PkgRequirementsValidator),
    EmbeddedPackage(EmbeddedPackageValidator),
}

/// For validating a package or recipe against a state
#[enum_dispatch]
pub trait ValidatorT {
    /// Check if the given package is appropriate for the provided state.
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Satisfy<PkgRequest> + Satisfy<VarRequest> + Package;

    /// Check if the given recipe is appropriate as a source build for the provided state.
    ///
    /// Once the build has been deemed resolvable and a binary package spec has
    /// been generated, the validate_package function will still be called and
    /// must be valid for the resulting build.
    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        _recipe: &R,
    ) -> crate::Result<Compatibility>;
}

/// For validating a spec against a pkg request
pub trait PkgRequestValidatorT {
    /// Check if the given package is appropriate for the pkg request
    fn validate_package_against_request<P>(
        &self,
        request: &PkgRequest,
        package: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: RecipeOps<PkgRequest = PkgRequest> + PackageOps<Ident = Ident> + Package;
}

impl PkgRequestValidatorT for Validators {
    fn validate_package_against_request<P>(
        &self,
        request: &PkgRequest,
        package: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: RecipeOps<PkgRequest = PkgRequest> + PackageOps<Ident = Ident> + Package,
    {
        match self {
            Validators::Deprecation(v) => {
                v.validate_package_against_request(request, package, source)
            }
            Validators::BinaryOnly(v) => {
                v.validate_package_against_request(request, package, source)
            }
            Validators::PackageRequest(v) => {
                v.validate_package_against_request(request, package, source)
            }
            Validators::Components(v) => {
                v.validate_package_against_request(request, package, source)
            }
            // The other validators don't implement this
            _ => Ok(Compatibility::Compatible),
        }
    }
}

/// Ensures that deprecated packages are not included unless specifically requested.
#[derive(Clone, Copy)]
pub struct DeprecationValidator {}

impl ValidatorT for DeprecationValidator {
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Package,
    {
        // TODO: is this going to be slower because it is making a
        // merged request that most of the time it will not need,
        // because the spec will be active (non-deprecated). The same
        // is true of some of the other validators. Profile it and see
        // how it looks. If it's a problem, we could move to merging
        // requests when they are added to the states and remove the
        // merge calls from the rest of the code.
        let request = state.get_merged_request(spec.name())?;
        self.validate_package_against_request(&request, spec, _source)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        recipe: &R,
    ) -> crate::Result<Compatibility> {
        if recipe.is_deprecated() {
            Ok(Compatibility::Incompatible(
                "recipe is deprecated for this version".to_owned(),
            ))
        } else {
            Ok(Compatibility::Compatible)
        }
    }
}

impl PkgRequestValidatorT for DeprecationValidator {
    fn validate_package_against_request<P>(
        &self,
        request: &PkgRequest,
        package: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: RecipeOps<PkgRequest = PkgRequest> + PackageOps<Ident = Ident> + Package,
    {
        if !package.is_deprecated() {
            return Ok(Compatibility::Compatible);
        }
        if package.ident().build.is_none() {
            return Ok(Compatibility::Incompatible(
                "package version is deprecated".to_owned(),
            ));
        }
        if request.pkg.build == package.ident().build {
            return Ok(Compatibility::Compatible);
        }
        Ok(Compatibility::Incompatible(
            "build is deprecated (and not requested exactly)".to_owned(),
        ))
    }
}

/// Enforces the resolution of binary packages only, denying new builds from source.
#[derive(Clone, Copy)]
pub struct BinaryOnlyValidator {}

impl ValidatorT for BinaryOnlyValidator {
    fn validate_package<P: Package>(
        &self,
        _state: &State,
        _spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility> {
        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        Ok(Compatibility::Incompatible(
            "building from source is not enabled".into(),
        ))
    }
}

impl PkgRequestValidatorT for BinaryOnlyValidator {
    fn validate_package_against_request<P>(
        &self,
        request: &PkgRequest,
        package: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: RecipeOps<PkgRequest = PkgRequest> + PackageOps<Ident = Ident> + Package,
    {
        if package.ident().build.is_none()
            || (package.ident().is_source() && request.pkg.build != package.ident().build)
        {
            return Ok(Compatibility::Incompatible(
                "building from source is not enabled".into(),
            ));
        }
        Ok(Compatibility::Compatible)
    }
}

#[derive(Clone, Copy)]
pub struct EmbeddedPackageValidator {}

impl ValidatorT for EmbeddedPackageValidator {
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Package,
    {
        for embedded in spec.embedded().iter() {
            let compat = Self::validate_embedded_package_against_state(spec, embedded, state)?;
            if !&compat {
                return Ok(compat);
            }
        }

        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        Ok(Compatibility::Compatible)
    }
}

impl EmbeddedPackageValidator {
    fn validate_embedded_package_against_state<P>(
        spec: &P,
        embedded: &Spec,
        state: &State,
    ) -> crate::Result<Compatibility>
    where
        P: Package,
    {
        use Compatibility::{Compatible, Incompatible};

        // There may not be a "real" instance of the embedded package in the
        // solve already.
        if let Some((existing, _)) = state.get_resolved_packages().get(embedded.ident().name()) {
            // If found, it must be the stub of the package now being embedded
            // to be okay.
            match existing.ident().build() {
                Build::Embedded(EmbeddedSource::Package(package))
                    if package.ident == spec.ident() => {}
                _ => {
                    return Ok(Incompatible(format!(
                        "embedded package '{}' conflicts with existing package in solve: {}",
                        embedded.ident(),
                        existing.ident()
                    )));
                }
            }
        }

        let existing = match state.get_merged_request(embedded.name()) {
            Ok(request) => request,
            Err(spk_solve_graph::GetMergedRequestError::NoRequestFor(_)) => return Ok(Compatible),
            Err(err) => return Err(err.into()),
        };

        let compat = existing.is_satisfied_by(embedded);
        if !&compat {
            return Ok(Incompatible(format!(
                "embedded package '{}' is incompatible: {compat}",
                embedded.ident(),
            )));
        }
        Ok(Compatible)
    }
}

/// Ensures that a package is compatible with all requested options.
#[derive(Clone, Copy, Default)]
pub struct OptionsValidator {}

impl ValidatorT for OptionsValidator {
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Package + Satisfy<VarRequest>,
    {
        let requests = state.get_var_requests();
        let qualified_requests: HashSet<_> = requests
            .iter()
            .filter_map(|r| {
                if r.var.namespace() == Some(spec.name()) {
                    Some(r.var.without_namespace())
                } else {
                    None
                }
            })
            .collect();
        for request in requests {
            if request.var.namespace().is_none() && qualified_requests.contains(&*request.var) {
                // a qualified request was found that supersedes this one:
                // eg: this is 'debug', but we have 'thispackage.debug'
                continue;
            }
            let compat = request.is_satisfied_by(spec);
            if !&compat {
                return Ok(Compatibility::Incompatible(format!(
                    "doesn't satisfy requested option: {compat}"
                )));
            }
        }
        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        state: &State,
        recipe: &R,
    ) -> crate::Result<Compatibility> {
        if let Err(err) = recipe.resolve_options(state.get_option_map()) {
            Ok(Compatibility::Incompatible(err.to_string()))
        } else {
            Ok(Compatibility::Compatible)
        }
    }
}

/// Ensures that a package meets all requested version criteria.
#[derive(Clone, Copy)]
pub struct PkgRequestValidator {}

impl ValidatorT for PkgRequestValidator {
    #[allow(clippy::nonminimal_bool)]
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Satisfy<PkgRequest> + Package,
    {
        let request = match state.get_merged_request(spec.name()) {
            Ok(request) => request,
            Err(GetMergedRequestError::NoRequestFor(name)) => {
                return Ok(Compatibility::Incompatible(format!(
                    "package '{name}' was not requested [INTERNAL ERROR]"
                )))
            }
            Err(err) => {
                return Ok(Compatibility::Incompatible(format!(
                    "package '{}' has an invalid request stack [INTERNAL ERROR]: {err}",
                    spec.name()
                )))
            }
        };
        self.validate_package_against_request(&request, spec, source)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        state: &State,
        recipe: &R,
    ) -> crate::Result<Compatibility> {
        let request = match state.get_merged_request(recipe.name()) {
            Ok(request) => request,
            Err(GetMergedRequestError::NoRequestFor(name)) => {
                return Ok(Compatibility::Incompatible(format!(
                    "package '{name}' was not requested [INTERNAL ERROR]"
                )))
            }
            Err(err) => {
                return Ok(Compatibility::Incompatible(format!(
                    "package '{}' has an invalid request stack [INTERNAL ERROR]: {err}",
                    recipe.name()
                )))
            }
        };
        Ok(request.is_version_applicable(recipe.version()))
    }
}

impl PkgRequestValidatorT for PkgRequestValidator {
    #[allow(clippy::nonminimal_bool)]
    fn validate_package_against_request<P>(
        &self,
        request: &PkgRequest,
        package: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: RecipeOps<PkgRequest = PkgRequest> + PackageOps<Ident = Ident> + Package,
    {
        if let Some(rn) = &request.pkg.repository_name {
            // If the request names a repository, then the source has to match.
            match source {
                PackageSource::Repository { repo, .. } if repo.name() != rn => {
                    return Ok(Compatibility::Incompatible(format!(
                        "package did not come from requested repo: {} != {}",
                        repo.name(),
                        rn
                    )));
                }
                PackageSource::Repository { .. } => {} // okay
                PackageSource::Embedded => {
                    // TODO: from the right repo still?
                    return Ok(Compatibility::Incompatible(
                        "package did not come from requested repo (it was embedded in another)"
                            .to_owned(),
                    ));
                }
                PackageSource::BuildFromSource { .. } => {
                    // TODO: from the right repo still?
                    return Ok(Compatibility::Incompatible(
                        "package did not come from requested repo (it comes from a spec)"
                            .to_owned(),
                    ));
                }
            };
        }
        // the initial check is more general and provides more user
        // friendly error messages that we'd like to get
        let mut compat = request.is_version_applicable(package.version());
        if !!&compat {
            compat = request.is_satisfied_by(package)
        }
        Ok(compat)
    }
}

/// Ensures that all of the requested components are available.
#[derive(Clone, Copy)]
pub struct ComponentsValidator {}

impl ValidatorT for ComponentsValidator {
    #[allow(clippy::nonminimal_bool)]
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Package,
    {
        use Compatibility::Compatible;

        if let Ok(Compatible) = self.check_for_embedded_stub(spec) {
            return Ok(Compatible);
        }

        let request = state.get_merged_request(spec.name())?;
        if let Ok(Compatibility::Incompatible(reason)) =
            self.check_for_missing_components(&request, spec, source)
        {
            return Ok(Compatibility::Incompatible(reason));
        }

        let required_components = spec
            .components()
            .resolve_uses(request.pkg.components.iter());

        for component in spec.components().iter() {
            if !required_components.contains(&component.name) {
                continue;
            }

            for embedded in component.embedded.iter() {
                let compat = EmbeddedPackageValidator::validate_embedded_package_against_state(
                    spec, embedded, state,
                )?;
                if !&compat {
                    return Ok(compat);
                }
            }
        }
        Ok(Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        Ok(Compatibility::Compatible)
    }
}

impl PkgRequestValidatorT for ComponentsValidator {
    fn validate_package_against_request<P>(
        &self,
        request: &PkgRequest,
        package: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: RecipeOps<PkgRequest = PkgRequest> + PackageOps<Ident = Ident> + Package,
    {
        use Compatibility::Compatible;
        if let Ok(Compatible) = self.check_for_embedded_stub(package) {
            return Ok(Compatible);
        }

        if let Ok(Compatibility::Incompatible(reason)) =
            self.check_for_missing_components(request, package, source)
        {
            return Ok(Compatibility::Incompatible(reason));
        }

        Ok(Compatible)
    }
}

impl ComponentsValidator {
    fn check_for_embedded_stub<P>(&self, package: &P) -> crate::Result<Compatibility>
    where
        P: PackageOps<Ident = Ident> + Package,
    {
        if package.ident().build().is_embedded() {
            // Allow embedded stubs to validate.
            return Ok(Compatibility::Compatible);
        }

        Ok(Compatibility::Incompatible(
            "Not an embedded package".to_string(),
        ))
    }

    fn check_for_missing_components<P>(
        &self,
        request: &PkgRequest,
        package: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: PackageOps<Ident = Ident> + Package,
    {
        // Do the components available in the package match those
        // required by the request?
        let available_components: std::collections::HashSet<_> = match source {
            PackageSource::Repository { components, .. } => components.keys().collect(),
            PackageSource::BuildFromSource { .. } => package.components().names(),
            PackageSource::Embedded => package.components().names(),
        };

        let required_components = package
            .components()
            .resolve_uses(request.pkg.components.iter());

        let missing_components: std::collections::HashSet<_> = required_components
            .iter()
            .filter(|n| !available_components.contains(n))
            .collect();

        if !missing_components.is_empty() {
            return Ok(Compatibility::Incompatible(format!(
                "no published files for some required components: [{}], found [{}]",
                required_components
                    .iter()
                    .map(Component::to_string)
                    .sorted()
                    .join(", "),
                available_components
                    .into_iter()
                    .map(Component::to_string)
                    .sorted()
                    .join(", ")
            )));
        }

        Ok(Compatibility::Compatible)
    }
}

/// Validates that the pkg install requirements do not conflict with the existing resolve.
#[derive(Clone, Copy)]
pub struct PkgRequirementsValidator {}

impl ValidatorT for PkgRequirementsValidator {
    fn validate_package<P: Package>(
        &self,
        state: &State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility> {
        for request in spec.runtime_requirements().iter() {
            let compat = self.validate_request_against_existing_state(state, request)?;
            if !&compat {
                return Ok(compat);
            }
        }

        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        // the recipe cannot tell us what the
        // runtime requirements will be
        Ok(Compatibility::Compatible)
    }
}

impl PkgRequirementsValidator {
    fn validate_request_against_existing_state(
        &self,
        state: &State,
        request: &Request,
    ) -> crate::Result<Compatibility> {
        use Compatibility::{Compatible, Incompatible};
        let request = match request {
            Request::Pkg(request) => request,
            _ => return Ok(Compatible),
        };

        let existing = match state.get_merged_request(&request.pkg.name) {
            Ok(request) => request,
            Err(spk_solve_graph::GetMergedRequestError::NoRequestFor(_)) => return Ok(Compatible),
            // XXX: KeyError or ValueError still possible here?
            Err(err) => return Err(err.into()),
        };

        let mut restricted = existing.clone();
        let request = match restricted.restrict(request) {
            Ok(_) => restricted,
            // FIXME: only match ValueError
            Err(spk_schema::ident::Error::String(err)) => {
                return Ok(Incompatible(format!("conflicting requirement: {err}")))
            }
            Err(err) => return Err(err.into()),
        };

        let (resolved, provided_components) = match state.get_current_resolve(&request.pkg.name) {
            Ok((spec, source)) => match source {
                PackageSource::Repository { components, .. } => (spec, components.keys().collect()),
                PackageSource::BuildFromSource { .. } | PackageSource::Embedded => {
                    (spec, spec.components().names())
                }
            },
            Err(spk_solve_graph::GetCurrentResolveError::PackageNotResolved(_)) => {
                return Ok(Compatible)
            }
        };

        let compat = Self::validate_request_against_existing_resolve(
            &request,
            resolved,
            provided_components,
        )?;
        if !&compat {
            return Ok(compat);
        }

        let existing_components = resolved
            .components()
            .resolve_uses(existing.pkg.components.iter());
        let required_components = resolved
            .components()
            .resolve_uses(request.pkg.components.iter());
        for component in resolved.components().iter() {
            if existing_components.contains(&component.name) {
                continue;
            }
            if !required_components.contains(&component.name) {
                continue;
            }
            for embedded in component.embedded.iter() {
                let compat = EmbeddedPackageValidator::validate_embedded_package_against_state(
                    &**resolved,
                    embedded,
                    state,
                )?;
                if !&compat {
                    return Ok(Compatibility::Incompatible(format!(
                        "requires {}:{} which embeds {}, and {}",
                        resolved.name(),
                        component.name,
                        embedded.name(),
                        compat,
                    )));
                }
            }
        }
        Ok(Compatible)
    }

    fn validate_request_against_existing_resolve(
        request: &PkgRequest,
        resolved: &CachedHash<std::sync::Arc<Spec>>,
        provided_components: std::collections::HashSet<&Component>,
    ) -> crate::Result<Compatibility> {
        use Compatibility::{Compatible, Incompatible};
        let compat = request.is_satisfied_by(&**resolved);
        if !&compat {
            return Ok(Incompatible(format!(
                "conflicting requirement: '{}' {}",
                request.pkg.name, compat
            )));
        }

        let required_components = resolved
            .components()
            .resolve_uses(request.pkg.components.iter());
        let missing_components: Vec<_> = required_components
            .iter()
            .filter(|c| !provided_components.contains(c))
            .collect();
        if !missing_components.is_empty() {
            return Ok(Incompatible(format!(
                "resolved package {} does not provide all required components: needed {}, have {}",
                request.pkg.name,
                missing_components
                    .into_iter()
                    .map(Component::to_string)
                    .join("\n"),
                {
                    if provided_components.is_empty() {
                        "none".to_owned()
                    } else {
                        provided_components
                            .into_iter()
                            .map(Component::to_string)
                            .join("\n")
                    }
                }
            )));
        }

        Ok(Compatible)
    }
}

/// Validates that the var install requirements do not conflict with the existing options.
#[derive(Clone, Copy, Default)]
pub struct VarRequirementsValidator {}

impl ValidatorT for VarRequirementsValidator {
    fn validate_package<P: Package>(
        &self,
        state: &State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility> {
        let options = state.get_option_map();
        for request in spec.runtime_requirements().iter() {
            if let Request::Var(request) = request {
                for (name, value) in options.iter() {
                    let is_not_requested = *name != request.var;
                    let is_not_same_base = request.var.base_name() != name.base_name();
                    if is_not_requested && is_not_same_base {
                        continue;
                    }
                    if value.is_empty() {
                        // empty option values do not provide a valuable opinion on the resolve
                        continue;
                    }
                    if request.value != *value {
                        return Ok(Compatibility::Incompatible(format!(
                            "package wants {}={}, resolve has {}={}",
                            request.var, request.value, name, value,
                        )));
                    }
                }
            }
        }
        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        // the recipe cannot tell us what the
        // runtime requirements will be
        Ok(Compatibility::Compatible)
    }
}

/// The default set of validators that is used for resolving packages
pub const fn default_validators() -> &'static [Validators] {
    // This controls the order the validators are checked
    &[
        Validators::Deprecation(DeprecationValidator {}),
        Validators::PackageRequest(PkgRequestValidator {}),
        Validators::Components(ComponentsValidator {}),
        Validators::Options(OptionsValidator {}),
        Validators::VarRequirements(VarRequirementsValidator {}),
        Validators::PkgRequirements(PkgRequirementsValidator {}),
        Validators::EmbeddedPackage(EmbeddedPackageValidator {}),
    ]
}

// The default set of validators that is used for impossible version request checks
pub const fn default_impossible_version_validators() -> &'static [Validators] {
    // The validators that detect issues with pkg version requests only.
    &[
        Validators::Deprecation(DeprecationValidator {}),
        Validators::PackageRequest(PkgRequestValidator {}),
        Validators::Components(ComponentsValidator {}),
    ]
}

/// Checks for impossible requests that a package would generate from
/// its install requirements and a set of unresolved requests.
/// Impossible and possible pkg requests are cached to speed up future
/// checking.
#[derive(Clone)]
pub struct ImpossibleRequestsChecker {
    /// The validators this uses to check for impossible requests
    validators: Cow<'static, [Validators]>,
    // TODO: because this just stores RangeIdents and not PkgRequests,
    // we don't have what made the requests, do we need this here,
    // should it be PkgRequests, at least for impossibles?
    /// Cache of impossible request to number of times it has been seen
    impossible_requests: HashMap<RangeIdent, u64>,
    /// Cache of possible request to number of times is has been seen
    possible_requests: HashMap<RangeIdent, u64>,
    /// Number of IfAlreadyPresent requests skipped during checks
    num_ifalreadypresent_requests: u64,
    /// Number of distinct impossible requests found
    num_impossible_requests_found: u64,
    /// Number of distinct possible requests found
    num_possible_requests_found: u64,
    /// Number of impossible requests found using the cache
    num_impossible_cache_hits: u64,
    /// Number of possible requests found using the cache
    num_possible_cache_hits: u64,
    /// Number of build specs read in during processing
    num_build_specs_read: u64,
}

impl Default for ImpossibleRequestsChecker {
    fn default() -> Self {
        Self {
            validators: Cow::from(default_impossible_version_validators()),
            impossible_requests: HashMap::new(),
            possible_requests: HashMap::new(),
            num_ifalreadypresent_requests: 0,
            num_impossible_requests_found: 0,
            num_possible_requests_found: 0,
            num_impossible_cache_hits: 0,
            num_possible_cache_hits: 0,
            num_build_specs_read: 0,
        }
    }
}

impl ImpossibleRequestsChecker {
    /// Set whether to only allow pre-built binary packages. If true,
    /// src packages will be treated as invalid for requests,
    /// otherwise src packages will be allowed to satisfy request checking
    pub fn set_binary_only(&mut self, binary_only: bool) {
        let has_binary_only = self
            .validators
            .iter()
            .find_map(|v| match v {
                Validators::BinaryOnly(_) => Some(true),
                _ => None,
            })
            .unwrap_or(false);
        if !(has_binary_only ^ binary_only) {
            return;
        }
        if binary_only {
            // Add BinaryOnly validator because it was missing.
            self.validators
                .to_mut()
                .insert(0, Validators::BinaryOnly(BinaryOnlyValidator {}))
        } else {
            // Remove all BinaryOnly validators because one was found.
            self.validators = take(self.validators.to_mut())
                .into_iter()
                .filter(|v| !matches!(v, Validators::BinaryOnly(_)))
                .collect();
        }
    }

    /// Reset the ImpossibleChecker's counters and request caches
    pub fn reset(&mut self) {
        self.impossible_requests.clear();
        self.possible_requests.clear();
        self.num_ifalreadypresent_requests = 0;
        self.num_impossible_requests_found = 0;
        self.num_possible_requests_found = 0;
        self.num_impossible_cache_hits = 0;
        self.num_possible_cache_hits = 0;
        self.num_build_specs_read = 0;
    }

    /// Get the impossible requests to frequency mapping
    pub fn get_impossible_requests(&self) -> &HashMap<RangeIdent, u64> {
        &self.impossible_requests
    }

    /// Get the possible requests to frequency mapping
    pub fn get_possible_requests(&self) -> &HashMap<RangeIdent, u64> {
        &self.possible_requests
    }

    /// Get the number of IfAlreadyPresent requests skipped during checks
    pub fn get_num_ifalreadypresent_requests(&self) -> u64 {
        self.num_ifalreadypresent_requests
    }

    /// Get the number of distinct impossible requests found
    pub fn get_num_impossible_requests_found(&self) -> u64 {
        self.num_impossible_requests_found
    }

    /// Get the number of distinct possible requests found
    pub fn get_num_possible_requests_found(&self) -> u64 {
        self.num_possible_requests_found
    }

    /// Get the number of impossible requests found using the cache
    pub fn get_num_impossible_hits(&self) -> u64 {
        self.num_impossible_cache_hits
    }

    /// Get the number of possible requests found using the cache
    pub fn get_num_possible_hits(&self) -> u64 {
        self.num_possible_cache_hits
    }

    /// Get the number of builds read in during processing so far
    pub fn get_num_build_specs_read(&self) -> u64 {
        self.num_build_specs_read
    }

    /// Check that the given package's install pkg requests are
    /// possible when combined with the unresolved requests.
    pub async fn validate_pkg_requests(
        &mut self,
        package: &Spec,
        unresolved_requests: &HashMap<PkgNameBuf, PkgRequest>,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<Compatibility> {
        let requirements: &RequirementsList = package.runtime_requirements();
        if requirements.is_empty() {
            return Ok(Compatibility::Compatible);
        }

        tracing::debug!(
            target: IMPOSSIBLE_REQUEST_TARGET,
            "{}: package has requirements: {}",
            package.ident(),
            requirements
                .iter()
                .filter_map(|r| match r {
                    Request::Pkg(pr) => Some(format!("{}", pr.pkg)),
                    _ => None,
                })
                .collect::<Vec<String>>()
                .join(", ")
        );

        for req in requirements.iter() {
            let request = match req {
                Request::Var(_) => {
                    // Any var requests are not part of these checks
                    continue;
                }
                Request::Pkg(r) => r,
            };

            tracing::debug!(
                target: IMPOSSIBLE_REQUEST_TARGET,
                "Build {} checking req: {}",
                package.ident(),
                request.pkg
            );

            // Generate the request that would be created if the
            // package was added to the unresolved requests.
            let combined_request = match unresolved_requests.get(&request.pkg.name) {
                None => request.clone(),
                Some(unresolved_request) => {
                    tracing::debug!(
                        target: IMPOSSIBLE_REQUEST_TARGET,
                        "Unresolved request: {}",
                        unresolved_request.pkg
                    );
                    let mut combined_request = request.clone();
                    combined_request.restrict(unresolved_request)?;
                    combined_request
                }
            };

            tracing::debug!(
                target: IMPOSSIBLE_REQUEST_TARGET,
                "Combined request: {combined_request} [{}]",
                combined_request.format_request(
                    &None,
                    &combined_request.pkg.name,
                    &FormatChangeOptions {
                        verbosity: 100,
                        level: 100
                    }
                )
            );

            if combined_request.inclusion_policy == InclusionPolicy::IfAlreadyPresent {
                // IfAlreadyPresent requests are optional until a
                // resolved package makes an actual dependency request
                // for that package. Until that happens they are
                // considered always possible.
                self.num_ifalreadypresent_requests += 1;
                tracing::debug!(
                    target: IMPOSSIBLE_REQUEST_TARGET,
                    "Combined request: {combined_request} has `IfAlreadyPresent` set, so it's possible"
                );
                continue;
            }

            if self.impossible_requests.contains_key(&combined_request.pkg) {
                // Previously found to be impossible, so return
                // incompatible immediately
                let counter = self
                    .impossible_requests
                    .entry(combined_request.pkg.clone())
                    .or_insert(0);
                *counter += 1;
                self.num_impossible_cache_hits += 1;

                tracing::debug!(
                    target: IMPOSSIBLE_REQUEST_TARGET,
                    "Matches cached Impossible request: denying {}",
                    combined_request.pkg
                );
                return Ok(Compatibility::Incompatible(format!(
                    "depends on {} which generates an impossible request {}",
                    request.pkg, combined_request.pkg
                )));
            }

            if self.possible_requests.contains_key(&combined_request.pkg) {
                // Previously found to be possible, move onto next requirement
                let counter = self
                    .possible_requests
                    .entry(combined_request.pkg.clone())
                    .or_insert(0);
                *counter += 1;
                self.num_possible_cache_hits += 1;

                tracing::debug!(
                    target: IMPOSSIBLE_REQUEST_TARGET,
                    "Matches cached Possible request: allowing {}",
                    combined_request.pkg
                );
                continue;
            }

            // Is there a valid build for the pkg request among all
            // the repos, versions, and builds?
            let any_valid = match self
                .any_build_valid_for_request(&combined_request, repos)
                .await
            {
                Ok(value) => value,
                Err(err) => return Err(err),
            };

            if any_valid {
                // Mark as possible and move on to the next request
                let counter = self
                    .possible_requests
                    .entry(combined_request.pkg.clone())
                    .or_insert(0);
                *counter += 1;
                self.num_possible_requests_found += 1;

                tracing::debug!(
                    target: IMPOSSIBLE_REQUEST_TARGET,
                    "Found Possible request, allowing and caching for next time: {}\n",
                    combined_request.pkg
                );
            } else {
                // Mark as impossible and immediately return as incompatible
                let counter = self
                    .impossible_requests
                    .entry(combined_request.pkg.clone())
                    .or_insert(0);
                *counter += 1;
                self.num_impossible_requests_found += 1;

                tracing::debug!(
                    target: IMPOSSIBLE_REQUEST_TARGET,
                    "Found Impossible request, denying and caching for next time: {}\n",
                    combined_request.pkg
                );

                return Ok(Compatibility::Incompatible(format!(
                    "depends on {} which generates an impossible request {}",
                    request.pkg, combined_request.pkg
                )));
            }
        }

        Ok(Compatibility::Compatible)
    }

    async fn get_build_components(
        &mut self,
        repo: &Arc<RepositoryHandle>,
        build: &Ident,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        // The correct spfs layer digest is not needed for the
        // validation that the Components validator does. So an empty
        // default digest is used to avoid calling read_components()
        // and the additional lookups it does (which would give the
        // correct digests).
        let mut components: HashMap<Component, spfs::encoding::Digest> = HashMap::new();
        match repo.list_build_components(build).await {
            Ok(v) => {
                for c in v.iter() {
                    components.insert(c.clone(), spfs::encoding::Digest::default());
                }
            }
            Err(spk_storage::Error::SpkValidatorsError(
                spk_schema::validators::Error::PackageNotFoundError(..),
            )) => {}
            Err(err) => return Err(Error::SpkStorageError(err)),
        };

        Ok(components)
    }

    /// Return true if there is any build in the repos that is valid
    /// for the given request, otherwise return false
    async fn any_build_valid_for_request(
        &mut self,
        combined_request: &PkgRequest,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<bool> {
        let base = Ident::from(combined_request.pkg.name.clone());

        for repo in repos.iter() {
            for version in repo.list_package_versions(&base.name).await?.iter() {
                let compat = combined_request.is_version_applicable(version);
                if !&compat {
                    // The version doesn't fit in the request range.
                    // So can skip all the builds in this version, as
                    // none of them will be valid for the request.
                    tracing::debug!(
                        target: IMPOSSIBLE_REQUEST_TARGET,
                        "version {version} isn't compat, skipping its builds: {compat}"
                    );
                    continue;
                }

                let pkg_version = base.with_version((**version).clone());

                // Note: because the builds aren't sorted, the order
                // they are returned in can vary from version to
                // version. That's okay this just needs to find one
                // that satisfies the request.
                let builds = repo.list_package_builds(&pkg_version).await?;
                for build in builds {
                    let spec = match repo.read_package(&build).await {
                        Ok(s) => s,
                        Err(err @ spk_storage::Error::InvalidPackageSpec(..)) => {
                            tracing::debug!(target: IMPOSSIBLE_REQUEST_TARGET, "Skipping: {err}");
                            continue;
                        }
                        Err(err) => return Err(Error::SpkStorageError(err)),
                    };

                    self.num_build_specs_read += 1;
                    tracing::debug!(
                        target: IMPOSSIBLE_REQUEST_TARGET,
                        "Read package spec for: {build} [compat={}]",
                        spec.compat()
                    );

                    // These are only needed for the Components validator
                    let components = self.get_build_components(repo, &build).await.unwrap();

                    let compat = self.validate_against_pkg_request(
                        combined_request,
                        &spec,
                        &PackageSource::Repository {
                            repo: repo.clone(),
                            components,
                        },
                    )?;

                    if !&compat {
                        // Not compatible, move on check the next build
                        tracing::debug!(
                            target: IMPOSSIBLE_REQUEST_TARGET,
                            "Invalid build {build} for the combined request: {compat}"
                        );
                    } else {
                        // Compatible with the request, which makes the
                        // request possible (or not impossible) and this
                        // doesn't need to look at anymore builds
                        tracing::debug!(
                            target: IMPOSSIBLE_REQUEST_TARGET,
                            "Found a valid build {build} for the combined request: {}",
                            combined_request.pkg
                        );
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    /// Reurn Compatible if the given spec is valid for the pkg
    /// request, otherwise return the Incompatible reason from the
    /// first validation check that failed.
    fn validate_against_pkg_request(
        &self,
        request: &PkgRequest,
        spec: &Spec,
        source: &PackageSource,
    ) -> Result<Compatibility> {
        for validator in self.validators.as_ref() {
            let compat = validator.validate_package_against_request(request, spec, source)?;
            if !&compat {
                return Ok(compat);
            }
        }
        Ok(Compatibility::Compatible)
    }
}
