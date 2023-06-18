// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use enum_dispatch::enum_dispatch;
use spk_schema::foundation::version::Compatibility;
use spk_schema::ident::{PkgRequest, Satisfy, VarRequest};
use spk_schema::name::PkgName;
use spk_schema::{Package, Recipe};
use spk_solve_graph::{GetMergedRequestResult, State};
use spk_solve_solution::PackageSource;

use crate::validators::*;
use crate::Error;

#[cfg(test)]
#[path = "./validation_test.rs"]
mod validation_test;

#[derive(Clone)]
#[enum_dispatch(ValidatorT)]
pub enum Validators {
    BinaryOnly(BinaryOnlyValidator),
    Components(ComponentsValidator),
    DenyPackageWithName(DenyPackageWithNameValidator),
    Deprecation(DeprecationValidator),
    EmbeddedPackage(EmbeddedPackageValidator),
    Options(OptionsValidator),
    PackageRequest(PkgRequestValidator),
    PkgRequirements(PkgRequirementsValidator),
    VarRequirements(VarRequirementsValidator),
}

/// For validation methods that only operate on package requests
pub trait GetMergedRequest {
    fn get_merged_request(&self, name: &PkgName) -> GetMergedRequestResult<PkgRequest>;
}

// This trait implementation for State is here because graph crate
// contains the State definition, and this (validation) crate uses the
// graph crate.
impl GetMergedRequest for State {
    fn get_merged_request(&self, name: &PkgName) -> GetMergedRequestResult<PkgRequest> {
        State::get_merged_request(self, name)
    }
}

/// For validating a package or recipe against various subsets of state data
#[enum_dispatch]
pub trait ValidatorT {
    /// Check if the given package is appropriate for the provided state data.
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Satisfy<PkgRequest> + Satisfy<VarRequest> + Package;

    /// Check if the given package is appropriate for the packages request data.
    ///
    /// This does not use options related data or data from already
    /// resolved parts of a state.
    fn validate_package_against_request<PR, P>(
        &self,
        _pkgrequest_data: &PR,
        _package: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        PR: GetMergedRequest,
        P: Satisfy<PkgRequest> + Package,
    {
        Err(Error::SolverError(
            "validate_package_against_request() is not implemented for this Validator".to_string(),
        ))
    }

    /// Check if the given recipe is appropriate as a source build for the provided state.
    ///
    /// Once the build has been deemed resolvable and a binary package spec has
    /// been generated, the validate_package function will still be called and
    /// must be valid for the resulting build.
    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        _recipe: &R,
    ) -> crate::Result<Compatibility>;
}

/// The default set of validators that is used for resolving packages
pub const fn default_validators() -> &'static [Validators] {
    // This controls the order the validators are checked
    &[
        Validators::Deprecation(DeprecationValidator {}),
        Validators::PackageRequest(PkgRequestValidator {}),
        Validators::Components(ComponentsValidator {}),
        Validators::Options(OptionsValidator {}),
        Validators::VarRequirements(VarRequirementsValidator {}),
        Validators::PkgRequirements(PkgRequirementsValidator {}),
        Validators::EmbeddedPackage(EmbeddedPackageValidator {}),
    ]
}
