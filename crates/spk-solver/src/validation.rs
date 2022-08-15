// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use enum_dispatch::enum_dispatch;
use itertools::Itertools;
use spk_ident::{Ident, PkgRequest, Request, VarRequest};
use spk_ident_component::Component;
use spk_spec::{Package, Recipe, Spec};
use spk_spec_ops::{Named, PackageOps, RecipeOps};
use spk_version::Compatibility;
use std::collections::HashSet;

use crate::error::GetMergedRequestError;

use super::{
    error,
    graph::{self, CachedHash},
    solution::PackageSource,
};

#[cfg(test)]
#[path = "./validation_test.rs"]
mod validation_test;

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

#[enum_dispatch]
pub trait ValidatorT {
    /// Check if the given package is appropriate for the provided state.
    fn validate_package<P>(
        &self,
        state: &graph::State,
        spec: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: RecipeOps<PkgRequest = PkgRequest>
            + PackageOps<Ident = Ident, VarRequest = VarRequest>
            + Package;

    /// Check if the given recipe is appropriate as a source build for the provided state.
    ///
    /// Once the build has been deemed resolvable and a binary package spec has
    /// been generated, the validate_package function will still be called and
    /// must be valid for the resulting build.
    fn validate_recipe<R: Recipe>(
        &self,
        _state: &graph::State,
        _recipe: &R,
    ) -> crate::Result<Compatibility>;
}

/// Ensures that deprecated packages are not included unless specifically requested.
#[derive(Clone, Copy)]
pub struct DeprecationValidator {}

impl ValidatorT for DeprecationValidator {
    fn validate_package<P>(
        &self,
        state: &graph::State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: PackageOps<Ident = Ident> + Package,
    {
        if !spec.is_deprecated() {
            return Ok(Compatibility::Compatible);
        }
        if spec.ident().build.is_none() {
            return Ok(Compatibility::Incompatible(
                "package version is deprecated".to_owned(),
            ));
        }
        let request = state.get_merged_request(spec.name())?;
        if request.pkg.build == spec.ident().build {
            return Ok(Compatibility::Compatible);
        }
        Ok(Compatibility::Incompatible(
            "build is deprecated (and not requested exactly)".to_owned(),
        ))
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &graph::State,
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

/// Enforces the resolution of binary packages only, denying new builds from source.
#[derive(Clone, Copy)]
pub struct BinaryOnlyValidator {}

impl ValidatorT for BinaryOnlyValidator {
    fn validate_package<P: Package>(
        &self,
        _state: &graph::State,
        _spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility> {
        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &graph::State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        Ok(Compatibility::Incompatible(
            "building from source is not enabled".into(),
        ))
    }
}

#[derive(Clone, Copy)]
pub struct EmbeddedPackageValidator {}

impl ValidatorT for EmbeddedPackageValidator {
    fn validate_package<P: Package>(
        &self,
        state: &graph::State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility> {
        for embedded in spec.embedded().iter() {
            let compat = Self::validate_embedded_package_against_state(embedded, state)?;
            if !&compat {
                return Ok(compat);
            }
        }

        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &graph::State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        Ok(Compatibility::Compatible)
    }
}

impl EmbeddedPackageValidator {
    fn validate_embedded_package_against_state(
        embedded: &Spec,
        state: &graph::State,
    ) -> crate::Result<Compatibility> {
        use Compatibility::{Compatible, Incompatible};
        let existing = match state.get_merged_request(embedded.name()) {
            Ok(request) => request,
            Err(error::GetMergedRequestError::NoRequestFor(_)) => return Ok(Compatible),
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
        state: &graph::State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: PackageOps<Ident = Ident, VarRequest = VarRequest> + Package,
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
                    "doesn't satisfy requested option: {}",
                    compat
                )));
            }
        }
        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        state: &graph::State,
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
        state: &graph::State,
        spec: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: RecipeOps<PkgRequest = PkgRequest>
            + PackageOps<Ident = Ident, VarRequest = VarRequest>
            + Package,
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
        let mut compat = request.is_version_applicable(spec.version());
        if !!&compat {
            compat = request.is_satisfied_by(spec)
        }
        Ok(compat)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        state: &graph::State,
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

/// Ensures that all of the requested components are available.
#[derive(Clone, Copy)]
pub struct ComponentsValidator {}

impl ValidatorT for ComponentsValidator {
    #[allow(clippy::nonminimal_bool)]
    fn validate_package<P: Package>(
        &self,
        state: &graph::State,
        spec: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility> {
        use Compatibility::Compatible;
        let available_components: std::collections::HashSet<_> = match source {
            PackageSource::Repository { components, .. } => components.keys().collect(),
            PackageSource::BuildFromSource { .. } => spec.components().names(),
            PackageSource::Embedded => spec.components().names(),
        };
        let request = state.get_merged_request(spec.name())?;
        let required_components = spec
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

        for component in spec.components().iter() {
            if !required_components.contains(&component.name) {
                continue;
            }

            for embedded in component.embedded.iter() {
                let compat = EmbeddedPackageValidator::validate_embedded_package_against_state(
                    embedded, state,
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
        _state: &graph::State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        Ok(Compatibility::Compatible)
    }
}

/// Validates that the pkg install requirements do not conflict with the existing resolve.
#[derive(Clone, Copy)]
pub struct PkgRequirementsValidator {}

impl ValidatorT for PkgRequirementsValidator {
    fn validate_package<P: Package>(
        &self,
        state: &graph::State,
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
        _state: &graph::State,
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
        state: &graph::State,
        request: &Request,
    ) -> crate::Result<Compatibility> {
        use Compatibility::{Compatible, Incompatible};
        let request = match request {
            Request::Pkg(request) => request,
            _ => return Ok(Compatible),
        };

        let existing = match state.get_merged_request(&request.pkg.name) {
            Ok(request) => request,
            Err(error::GetMergedRequestError::NoRequestFor(_)) => return Ok(Compatible),
            // XXX: KeyError or ValueError still possible here?
            Err(err) => return Err(err.into()),
        };

        let mut restricted = existing.clone();
        let request = match restricted.restrict(request) {
            Ok(_) => restricted,
            // FIXME: only match ValueError
            Err(spk_ident::Error::String(err)) => {
                return Ok(Incompatible(format!("conflicting requirement: {}", err)))
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
            Err(error::GetCurrentResolveError::PackageNotResolved(_)) => return Ok(Compatible),
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
        for component in resolved.components_iter() {
            if existing_components.contains(&component.name) {
                continue;
            }
            if !required_components.contains(&component.name) {
                continue;
            }
            for embedded in component.embedded.iter() {
                let compat = EmbeddedPackageValidator::validate_embedded_package_against_state(
                    embedded, state,
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
                "resolved package does not provide all required components: needed {}, have {}",
                missing_components
                    .into_iter()
                    .map(Component::to_string)
                    .join("\n"),
                request.pkg.name,
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
        state: &graph::State,
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
        _state: &graph::State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        // the recipe cannot tell us what the
        // runtime requirements will be
        Ok(Compatibility::Compatible)
    }
}

/// The default set of validators that is used for resolving packages
pub const fn default_validators() -> &'static [Validators] {
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
