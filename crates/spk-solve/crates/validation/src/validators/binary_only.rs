// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spk_schema::version::IncompatibleReason;

use super::prelude::*;
use crate::ValidatorT;

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
            IncompatibleReason::BuildFromSourceDisabled,
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
        P: Satisfy<PkgRequestWithOptions> + Package,
    {
        let request = pkgrequest_data.get_merged_request(package.name())?;
        if package.ident().is_source()
            && request.pkg.build.as_ref() != Some(package.ident().build())
        {
            return Ok(Compatibility::Incompatible(
                IncompatibleReason::BuildFromSourceDisabled,
            ));
        }
        Ok(Compatibility::Compatible)
    }
}
