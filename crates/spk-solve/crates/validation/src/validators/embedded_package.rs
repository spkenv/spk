// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::prelude::*;
use crate::ValidatorT;

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
        // Only the embedded packages that are active based on the components
        // being requested from this spec are checked. There may be
        // incompatibilities with other embedded packages that are not active
        // but that shouldn't block this spec from being valid.
        let request = state.get_merged_request(spec.name())?;

        // XXX: Can `request.pkg.components` be empty? What would that mean?
        debug_assert!(
            !request.pkg.components.is_empty(),
            "empty request.pkg.components not handled"
        );

        Self::validate_embedded_packages_in_required_components(spec, &request, state)
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
    pub(crate) fn validate_embedded_package_against_state<P>(
        spec: &P,
        embedded: &Spec,
        state: &State,
    ) -> crate::Result<Compatibility>
    where
        P: Package,
    {
        use Compatibility::Compatible;

        // There must not be a "real" instance of the embedded package in the
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

    /// Validate that any embedded packages that are present in any of the
    /// components requested by the given request are compatible with the
    /// current state.
    pub(super) fn validate_embedded_packages_in_required_components<P>(
        spec: &P,
        request: &PkgRequest,
        state: &State,
    ) -> crate::Result<Compatibility>
    where
        P: Package,
    {
        let required_components = spec
            .components()
            .resolve_uses(request.pkg.components.iter());

        for component in spec.components().iter() {
            // If the component is not active, skip it.
            if !required_components.contains(&component.name) {
                continue;
            }

            for embedded_package in component.embedded_packages.iter() {
                for embedded in spec
                    .embedded()
                    .packages_matching_embedded_package(embedded_package)
                {
                    let compat =
                        Self::validate_embedded_package_against_state(spec, embedded, state)?;
                    if !&compat {
                        return Ok(compat);
                    }
                }
            }
        }

        Ok(Compatibility::Compatible)
    }
}
