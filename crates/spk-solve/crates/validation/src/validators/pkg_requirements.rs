// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use itertools::Itertools;

use super::prelude::*;
use crate::validators::EmbeddedPackageValidator;
use crate::ValidatorT;

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
