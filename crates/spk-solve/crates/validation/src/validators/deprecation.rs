// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::prelude::*;
use crate::ValidatorT;

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
