// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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
