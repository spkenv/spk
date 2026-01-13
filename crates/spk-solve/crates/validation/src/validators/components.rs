// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::BTreeSet;

use spk_schema::ident::AsVersionIdent;
use spk_schema::version::{CommaSeparated, ComponentsMissingProblem, IncompatibleReason};

use super::prelude::*;
use crate::ValidatorT;
use crate::validators::EmbeddedPackageValidator;

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
        <P as Package>::EmbeddedPackage: AsVersionIdent + Named + Satisfy<PkgRequestWithOptions>,
    {
        use Compatibility::Compatible;

        if let Ok(Compatible) = self.check_for_embedded_stub(spec) {
            return Ok(Compatible);
        }

        let request = state.get_merged_request(spec.name())?;
        if let Ok(Compatibility::Incompatible(reason)) =
            self.check_for_missing_components(&request.pkg_request, spec, source)
        {
            return Ok(Compatibility::Incompatible(reason));
        }

        EmbeddedPackageValidator::validate_embedded_packages_in_required_components(
            spec, &request, state,
        )
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
            self.check_for_missing_components(&request.pkg_request, package, source)
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

        Ok(Compatibility::Incompatible(
            IncompatibleReason::PackageNotAnEmbeddedPackage,
        ))
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
            PackageSource::Embedded { components, .. } => components.iter().collect(),
            PackageSource::SpkInternalTest => package.components().names(),
        };

        let required_components = package
            .components()
            .resolve_uses(request.pkg.components.iter());

        let missing_components: std::collections::BTreeSet<_> = required_components
            .iter()
            .filter(|&n| !available_components.contains(n))
            .map(|n| n.to_string())
            .collect();

        if !missing_components.is_empty() {
            return Ok(Compatibility::Incompatible(
                IncompatibleReason::ComponentsMissing(
                    ComponentsMissingProblem::ComponentsNotDefined {
                        missing: CommaSeparated(missing_components),
                        available: CommaSeparated(BTreeSet::<String>::from_iter(
                            available_components.into_iter().map(|s| s.to_string()),
                        )),
                    },
                ),
            ));
        }

        Ok(Compatibility::Compatible)
    }
}
