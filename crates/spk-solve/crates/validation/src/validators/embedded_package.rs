// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spk_schema::version::IncompatibleReason;

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
    pub(crate) fn validate_embedded_package_against_state<P>(
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

        if let Compatibility::Incompatible(incompatible) = existing.is_satisfied_by(embedded) {
            return Ok(Compatibility::Incompatible(
                IncompatibleReason::EmbeddedIncompatible {
                    pkg: embedded.ident().to_string(),
                    inner_reason: Box::new(incompatible),
                },
            ));
        }
        Ok(Compatible)
    }
}
