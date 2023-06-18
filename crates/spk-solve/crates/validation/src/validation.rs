// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use enum_dispatch::enum_dispatch;
use itertools::Itertools;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::spec_ops::Named;
use spk_schema::foundation::version::Compatibility;
use spk_schema::ident::{PkgRequest, Request, Satisfy, VarRequest};
use spk_schema::ident_build::{Build, EmbeddedSource};
use spk_schema::name::PkgName;
use spk_schema::{Package, Recipe, Spec};
use spk_solve_graph::{CachedHash, GetMergedRequestError, GetMergedRequestResult, State};
use spk_solve_solution::PackageSource;

use crate::Error;

#[cfg(test)]
#[path = "./validation_test.rs"]
mod validation_test;

#[derive(Clone, Copy)]
#[enum_dispatch(ValidatorT)]
pub enum Validators {
    BinaryOnly(BinaryOnlyValidator),
    Components(ComponentsValidator),
    Deprecation(DeprecationValidator),
    EmbeddedPackage(EmbeddedPackageValidator),
    Options(OptionsValidator),
    PackageRequest(PkgRequestValidator),
    PkgRequirements(PkgRequirementsValidator),
    VarRequirements(VarRequirementsValidator),
}

/// For validation methods that only operate on package requests
pub trait GetMergedRequest {
    fn get_merged_request(&self, name: &PkgName) -> GetMergedRequestResult<PkgRequest>;
}

// This trait implementation for State is here because graph crate
// contains the State definition, and this (validation) crate uses the
// graph crate.
impl GetMergedRequest for State {
    fn get_merged_request(&self, name: &PkgName) -> GetMergedRequestResult<PkgRequest> {
        State::get_merged_request(self, name)
    }
}

/// For validating a package or recipe against various subsets of state data
#[enum_dispatch]
pub trait ValidatorT {
    /// Check if the given package is appropriate for the provided state data.
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Satisfy<PkgRequest> + Satisfy<VarRequest> + Package;

    /// Check if the given package is appropriate for the packages request data.
    ///
    /// This does not use options related data or data from already
    /// resolved parts of a state.
    fn validate_package_against_request<PR, P>(
        &self,
        _pkgrequest_data: &PR,
        _package: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        PR: GetMergedRequest,
        P: Satisfy<PkgRequest> + Package,
    {
        Err(Error::SolverError(
            "validate_package_against_request() is not implemented for this Validator".to_string(),
        ))
    }

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
        P: Satisfy<PkgRequest> + Satisfy<VarRequest> + Package,
    {
        self.validate_package_against_request(state, spec, _source)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        recipe: &R,
    ) -> crate::Result<Compatibility> {
        if recipe.is_deprecated() {
            Ok(Compatibility::incompatible(
                "recipe is deprecated for this version".to_owned(),
            ))
        } else {
            Ok(Compatibility::Compatible)
        }
    }

    fn validate_package_against_request<PR, P>(
        &self,
        pkgrequest_data: &PR,
        package: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        PR: GetMergedRequest,
        P: Satisfy<PkgRequest> + Package,
    {
        if !package.is_deprecated() {
            return Ok(Compatibility::Compatible);
        }
        let request = pkgrequest_data.get_merged_request(package.name())?;
        if request.pkg.build.as_ref() == Some(package.ident().build()) {
            return Ok(Compatibility::Compatible);
        }
        Ok(Compatibility::incompatible(
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
        Ok(Compatibility::incompatible(
            "building from source is not enabled",
        ))
    }

    fn validate_package_against_request<PR, P>(
        &self,
        pkgrequest_data: &PR,
        package: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        PR: GetMergedRequest,
        P: Satisfy<PkgRequest> + Package,
    {
        let request = pkgrequest_data.get_merged_request(package.name())?;
        if package.ident().is_source()
            && request.pkg.build.as_ref() != Some(package.ident().build())
        {
            return Ok(Compatibility::incompatible(
                "building from source is not enabled",
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
        use Compatibility::Compatible;

        // There may not be a "real" instance of the embedded package in the
        // solve already.
        if let Some((existing, _, _)) = state.get_resolved_packages().get(embedded.name()) {
            // If found, it must be the stub of the package now being embedded
            // to be okay.
            match existing.ident().build() {
                Build::Embedded(EmbeddedSource::Package(package))
                    if package.ident == spec.ident() => {}
                _ => {
                    return Ok(Compatibility::embedded_conflict(existing.name().to_owned()));
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
            return Ok(Compatibility::incompatible(format!(
                "embedded package '{}' is incompatible: {compat}",
                embedded.ident()
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
                return Ok(Compatibility::incompatible(format!(
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
            Ok(Compatibility::incompatible(err.to_string()))
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
        self.validate_package_against_request(state, spec, source)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        state: &State,
        recipe: &R,
    ) -> crate::Result<Compatibility> {
        let request = match state.get_merged_request(recipe.name()) {
            Ok(request) => request,
            Err(GetMergedRequestError::NoRequestFor(name)) => {
                return Ok(Compatibility::incompatible(format!(
                    "package '{name}' was not requested [INTERNAL ERROR]"
                )))
            }
            Err(err) => {
                return Ok(Compatibility::incompatible(format!(
                    "package '{}' has an invalid request stack [INTERNAL ERROR]: {err}",
                    recipe.name()
                )))
            }
        };
        Ok(request.is_version_applicable(recipe.version()))
    }

    #[allow(clippy::nonminimal_bool)]
    fn validate_package_against_request<PR, P>(
        &self,
        pkgrequest_data: &PR,
        package: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Satisfy<PkgRequest> + Package,
        PR: GetMergedRequest,
    {
        let request = match pkgrequest_data.get_merged_request(package.name()) {
            Ok(request) => request,
            Err(GetMergedRequestError::NoRequestFor(name)) => {
                return Ok(Compatibility::incompatible(format!(
                    "package '{name}' was not requested [INTERNAL ERROR]"
                )))
            }
            Err(err) => {
                return Ok(Compatibility::incompatible(format!(
                    "package '{}' has an invalid request stack [INTERNAL ERROR]: {err}",
                    package.name()
                )))
            }
        };

        if let Some(rn) = &request.pkg.repository_name {
            // If the request names a repository, then the source has to match.
            match source {
                PackageSource::Repository { repo, .. } if repo.name() != rn => {
                    return Ok(Compatibility::incompatible(format!(
                        "package did not come from requested repo: {} != {}",
                        repo.name(),
                        rn
                    )));
                }
                PackageSource::Repository { .. } => {} // okay
                PackageSource::Embedded { parent } => {
                    // TODO: from the right repo still?
                    return Ok(Compatibility::incompatible(format!(
                        "package did not come from requested repo (it was embedded in {parent})"
                    )));
                }
                PackageSource::BuildFromSource { .. } => {
                    // TODO: from the right repo still?
                    return Ok(Compatibility::incompatible(
                        "package did not come from requested repo (it comes from a spec)"
                            .to_owned(),
                    ));
                }
                PackageSource::SpkInternalTest => {
                    return Ok(Compatibility::incompatible(
                        "package did not come from requested repo (it comes from an internal test setup)"
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

    fn validate_package_against_request<PR, P>(
        &self,
        pkgrequest_data: &PR,
        package: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        PR: GetMergedRequest,
        P: Package,
    {
        use Compatibility::Compatible;
        if let Ok(Compatible) = self.check_for_embedded_stub(package) {
            return Ok(Compatible);
        }

        let request = pkgrequest_data.get_merged_request(package.name())?;
        if let Ok(Compatibility::Incompatible(reason)) =
            self.check_for_missing_components(&request, package, source)
        {
            return Ok(Compatibility::Incompatible(reason));
        }

        Ok(Compatible)
    }
}

impl ComponentsValidator {
    fn check_for_embedded_stub<P>(&self, package: &P) -> crate::Result<Compatibility>
    where
        P: Package,
    {
        if package.ident().build().is_embedded() {
            // Allow embedded stubs to validate.
            return Ok(Compatibility::Compatible);
        }

        Ok(Compatibility::incompatible("Not an embedded package"))
    }

    fn check_for_missing_components<P>(
        &self,
        request: &PkgRequest,
        package: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Package,
    {
        // Do the components available in the package match those
        // required by the request?
        let available_components: std::collections::HashSet<_> = match source {
            PackageSource::Repository { components, .. } => components.keys().collect(),
            PackageSource::BuildFromSource { .. } => package.components().names(),
            PackageSource::Embedded { .. } => package.components().names(),
            PackageSource::SpkInternalTest => package.components().names(),
        };

        let required_components = package
            .components()
            .resolve_uses(request.pkg.components.iter());

        let missing_components: std::collections::HashSet<_> = required_components
            .iter()
            .filter(|n| !available_components.contains(n))
            .collect();

        if !missing_components.is_empty() {
            return Ok(Compatibility::incompatible(format!(
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
        use Compatibility::Compatible;
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
                return Ok(Compatibility::incompatible(format!(
                    "conflicting requirement: {err}"
                )))
            }
            Err(err) => return Err(err.into()),
        };

        let (resolved, provided_components) = match state.get_current_resolve(&request.pkg.name) {
            Ok((spec, source, _)) => match source {
                PackageSource::Repository { components, .. } => (spec, components.keys().collect()),
                PackageSource::BuildFromSource { .. }
                | PackageSource::Embedded { .. }
                | PackageSource::SpkInternalTest => (spec, spec.components().names()),
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
                    return Ok(Compatibility::incompatible(format!(
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
        use Compatibility::Compatible;
        let compat = request.is_satisfied_by(&**resolved);
        if !&compat {
            return Ok(Compatibility::incompatible(format!(
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
            return Ok(Compatibility::incompatible(format!(
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
                        return Ok(Compatibility::incompatible(format!(
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
