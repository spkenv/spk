// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use itertools::Itertools;

use super::prelude::*;
use crate::validators::EmbeddedPackageValidator;
use crate::ValidatorT;

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
