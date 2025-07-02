// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spk_schema::version::{
    CommaSeparated,
    ComponentsMissingProblem,
    ConflictingRequirementProblem,
    IncompatibleReason,
};

use super::prelude::*;
use crate::ValidatorT;
use crate::validators::EmbeddedPackageValidator;

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
            Compatible => restricted,
            Compatibility::Incompatible(incompatible) => {
                return Ok(Compatibility::Incompatible(
                    IncompatibleReason::ConflictingRequirement(
                        ConflictingRequirementProblem::PkgRequirement(Box::new(incompatible)),
                    ),
                ));
            }
        };

        let mut was_embedded = None;

        let (resolved, provided_components) = match state.get_current_resolve(&request.pkg.name) {
            Ok((spec, source, _)) => match source {
                PackageSource::Repository { components, .. } => (spec, components.keys().collect()),
                PackageSource::Embedded { parent, components } => {
                    was_embedded = Some(parent);
                    (spec, components.iter().collect())
                }
                PackageSource::BuildFromSource { .. } | PackageSource::SpkInternalTest => {
                    (spec, spec.components().names())
                }
            },
            Err(spk_solve_graph::GetCurrentResolveError::PackageNotResolved(_)) => {
                return Ok(Compatible);
            }
        };

        let compat = match Self::validate_request_against_existing_resolve(
            &request,
            resolved,
            provided_components,
        )? {
            Compatible => Compatible,
            Compatibility::Incompatible(reason) => match (reason, was_embedded) {
                (
                    IncompatibleReason::ComponentsMissing(
                        ComponentsMissingProblem::ComponentsNotProvided {
                            package,
                            needed,
                            have,
                        },
                    ),
                    Some(parent),
                ) => Compatibility::Incompatible(IncompatibleReason::ComponentsMissing(
                    ComponentsMissingProblem::EmbeddedComponentsNotProvided {
                        embedder: parent.name().to_owned(),
                        embedded: package,
                        needed,
                        have,
                    },
                )),
                (reason, _) => Compatibility::Incompatible(reason),
            },
        };
        if !compat.is_ok() {
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
            for embedded_package in component.embedded.iter() {
                for embedded in resolved
                    .embedded()
                    .packages_matching_embedded_package(embedded_package)
                {
                    let compat = EmbeddedPackageValidator::validate_embedded_package_against_state(
                        &**resolved,
                        embedded,
                        state,
                    )?;
                    if let Compatibility::Incompatible(compat) = compat {
                        return Ok(Compatibility::Incompatible(
                            IncompatibleReason::ConflictingEmbeddedPackageRequirement(
                                resolved.name().to_owned(),
                                component.name.to_string(),
                                embedded.name().to_owned(),
                                Box::new(compat),
                            ),
                        ));
                    }
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
        if let Compatibility::Incompatible(compat) = compat {
            return Ok(Compatibility::Incompatible(
                IncompatibleReason::ConflictingRequirement(
                    ConflictingRequirementProblem::ExistingPackage {
                        pkg: request.pkg.name.clone(),
                        inner_reason: Box::new(compat),
                    },
                ),
            ));
        }
        let required_components = resolved
            .components()
            .resolve_uses(request.pkg.components.iter());
        let missing_components: Vec<_> = required_components
            .iter()
            .filter(|c| !provided_components.contains(c))
            .collect();
        if !missing_components.is_empty() {
            return Ok(Compatibility::Incompatible(
                IncompatibleReason::ComponentsMissing(
                    ComponentsMissingProblem::ComponentsNotProvided {
                        package: request.pkg.name.to_owned(),
                        needed: CommaSeparated(
                            missing_components
                                .into_iter()
                                .map(|c| c.to_string())
                                .collect(),
                        ),
                        have: CommaSeparated(
                            provided_components
                                .into_iter()
                                .map(|c| c.to_string())
                                .collect(),
                        ),
                    },
                ),
            ));
        }

        Ok(Compatible)
    }
}
