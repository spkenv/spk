// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema::name::{PkgName, PkgNameBuf};

use super::prelude::*;
use crate::{ValidatorT, Validators};

/// Ensures that packages with the given name are not included.
#[derive(Clone)]
pub struct DenyPackageWithNameValidator {
    pub name: PkgNameBuf,
}

impl DenyPackageWithNameValidator {
    /// Check that the given name is not the name being denied.
    fn check_name(&self, name: &PkgName) -> crate::Result<Compatibility> {
        if name == self.name {
            Ok(Compatibility::incompatible(format!(
                "package with name {} is not allowed",
                self.name
            )))
        } else {
            Ok(Compatibility::Compatible)
        }
    }

    /// Add or remove a DenyPackageWithName validator to the given list of
    /// validators.
    pub fn update_validators(pkg_name: &PkgName, reject: bool, validators: &mut Vec<Validators>) {
        let has_reject = validators
            .iter()
            .find_map(|v| match v {
                Validators::DenyPackageWithName(DenyPackageWithNameValidator { name })
                    if name == pkg_name =>
                {
                    Some(true)
                }
                _ => None,
            })
            .unwrap_or(false);
        if !(has_reject ^ reject) {
            return;
        }
        if reject {
            // Add validator because it was missing.
            validators.insert(
                0,
                Validators::DenyPackageWithName(DenyPackageWithNameValidator {
                    name: pkg_name.to_owned(),
                }),
            )
        } else {
            // Remove any DenyPackageWithName validators because one was found.
            validators
                .retain(|v| !matches!(v, Validators::DenyPackageWithName(DenyPackageWithNameValidator { name }) if name == pkg_name))
        }
    }
}

impl ValidatorT for DenyPackageWithNameValidator {
    fn validate_package<P>(
        &self,
        _state: &State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Named,
    {
        self.check_name(spec.name())
    }

    fn validate_package_against_request<PR, P>(
        &self,
        _pkgrequest_data: &PR,
        package: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        PR: GetMergedRequest,
        P: Satisfy<PkgRequest> + Package,
    {
        self.check_name(package.name())
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        Ok(Compatibility::Compatible)
    }
}
