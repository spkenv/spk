// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;

use crate::api::{self, Build};

use super::errors;
use super::graph;

#[derive(Clone, Copy)]
pub enum Validators {
    Deprecation(DeprecationValidator),
    BinaryOnly(BinaryOnlyValidator),
    PackageRequest(PkgRequestValidator),
    Options(OptionsValidator),
    VarRequirements(VarRequirementsValidator),
    PkgRequirements(PkgRequirementsValidator),
    EmbeddedPackage(EmbeddedPackageValidator),
}

pub trait ValidatorT {
    /// Check if the given package is appropriate for the provided state.
    fn validate(&self, state: &graph::State, spec: &api::Spec)
        -> crate::Result<api::Compatibility>;
}

impl ValidatorT for Validators {
    fn validate(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        match self {
            Validators::Deprecation(v) => v.validate(state, spec),
            Validators::BinaryOnly(v) => v.validate(state, spec),
            Validators::PackageRequest(v) => v.validate(state, spec),
            Validators::Options(v) => v.validate(state, spec),
            Validators::VarRequirements(v) => v.validate(state, spec),
            Validators::PkgRequirements(v) => v.validate(state, spec),
            Validators::EmbeddedPackage(v) => v.validate(state, spec),
        }
    }
}

impl IntoPy<Py<PyAny>> for Validators {
    fn into_py(self, py: Python) -> Py<PyAny> {
        match self {
            Validators::Deprecation(v) => v.into_py(py),
            Validators::BinaryOnly(v) => v.into_py(py),
            Validators::PackageRequest(v) => v.into_py(py),
            Validators::Options(v) => v.into_py(py),
            Validators::VarRequirements(v) => v.into_py(py),
            Validators::PkgRequirements(v) => v.into_py(py),
            Validators::EmbeddedPackage(v) => v.into_py(py),
        }
    }
}

/// Ensures that deprecated packages are not included unless specifically requested.
#[pyclass]
#[derive(Clone, Copy)]
pub struct DeprecationValidator {}

#[pymethods]
impl DeprecationValidator {
    #[pyo3(name = "validate")]
    fn validatepy(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        self.validate(state, spec)
    }
}

impl ValidatorT for DeprecationValidator {
    fn validate(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        if !spec.deprecated {
            return Ok(api::Compatibility::Compatible);
        }
        if spec.pkg.build.is_none() {
            return Ok(api::Compatibility::Incompatible(
                "package version is deprecated".to_owned(),
            ));
        }
        let request = state.get_merged_request(spec.pkg.name())?;
        if request.pkg.build == spec.pkg.build {
            return Ok(api::Compatibility::Compatible);
        }
        Ok(api::Compatibility::Incompatible(
            "build is deprecated (and not requested exactly)".to_owned(),
        ))
    }
}

/// Enforces the resolution of binary packages only, denying new builds from source.
#[pyclass]
#[derive(Clone, Copy)]
pub struct BinaryOnlyValidator {}

const ONLY_BINARY_PACKAGES_ALLOWED: &str = "only binary packages are allowed";

impl ValidatorT for BinaryOnlyValidator {
    fn validate(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        if spec.pkg.build.is_none() {
            return Ok(api::Compatibility::Incompatible(
                ONLY_BINARY_PACKAGES_ALLOWED.to_owned(),
            ));
        }
        let request = state.get_merged_request(spec.pkg.name())?;
        if spec.pkg.build == Some(Build::Source) && request.pkg.build != spec.pkg.build {
            return Ok(api::Compatibility::Incompatible(
                ONLY_BINARY_PACKAGES_ALLOWED.to_owned(),
            ));
        }
        Ok(api::Compatibility::Compatible)
    }
}

#[pyclass]
#[derive(Clone, Copy)]
pub struct EmbeddedPackageValidator {}

#[pymethods]
impl EmbeddedPackageValidator {
    #[pyo3(name = "validate")]
    fn validatepy(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        self.validate(state, spec)
    }
}

impl ValidatorT for EmbeddedPackageValidator {
    fn validate(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        if spec.pkg.is_source() {
            // source packages are not being "installed" so requests don't matter
            return Ok(api::Compatibility::Compatible);
        }

        for embedded in &spec.install.embedded {
            let existing = match state.get_merged_request(embedded.pkg.name()) {
                Ok(request) => request,
                Err(errors::GetMergedRequestError::NoRequestFor(_)) => continue,
                Err(err) => return Err(err.into()),
            };

            let compat = existing.is_satisfied_by(embedded);
            if !&compat {
                return Ok(api::Compatibility::Incompatible(format!(
                    "embedded package '{}' is incompatible: {}",
                    embedded.pkg, compat
                )));
            }
        }

        Ok(api::Compatibility::Compatible)
    }
}

/// Ensures that a package is compatible with all requested options.
#[pyclass]
#[derive(Clone, Copy)]
pub struct OptionsValidator {}

#[pymethods]
impl OptionsValidator {
    #[pyo3(name = "validate")]
    fn validatepy(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        self.validate(state, spec)
    }
}

impl ValidatorT for OptionsValidator {
    fn validate(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        for request in state.get_var_requests() {
            let compat = spec.satisfies_var_request(request);
            if !&compat {
                return Ok(api::Compatibility::Incompatible(format!(
                    "doesn't satisfy requested option: {}",
                    compat
                )));
            }
        }
        Ok(api::Compatibility::Compatible)
    }
}

/// Ensures that a package meets all requested version criteria.
#[pyclass]
#[derive(Clone, Copy)]
pub struct PkgRequestValidator {}

#[pymethods]
impl PkgRequestValidator {
    #[pyo3(name = "validate")]
    fn validatepy(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        self.validate(state, spec)
    }
}

impl ValidatorT for PkgRequestValidator {
    #[allow(clippy::nonminimal_bool)]
    fn validate(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        let request = match state.get_merged_request(spec.pkg.name()) {
            Ok(request) => request,
            // FIXME: This should only catch KeyError
            Err(_) => {
                return Ok(api::Compatibility::Incompatible(
                    "package was not requested [INTERNAL ERROR]".to_owned(),
                ))
            }
        };
        // the initial check is more general and provides more user
        // friendly error messages that we'd like to get
        let mut compat = request.is_version_applicable(&spec.pkg.version);
        if !!&compat {
            compat = request.is_satisfied_by(spec)
        }
        Ok(compat)
    }
}

/// Validates that the pkg install requirements do not conflict with the existing resolve.
#[pyclass]
#[derive(Clone, Copy)]
pub struct PkgRequirementsValidator {}

#[pymethods]
impl PkgRequirementsValidator {
    #[pyo3(name = "validate")]
    fn validatepy(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        self.validate(state, spec)
    }
}

impl ValidatorT for PkgRequirementsValidator {
    fn validate(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        if spec.pkg.is_source() {
            // source packages are not being "installed" so requests don't matter
            return Ok(api::Compatibility::Compatible);
        }

        for request in &spec.install.requirements {
            if let api::Request::Pkg(request) = request {
                let mut existing = match state.get_merged_request(request.pkg.name()) {
                    Ok(request) => request,
                    Err(errors::GetMergedRequestError::NoRequestFor(_)) => continue,
                    // XXX: KeyError or ValueError still possible here?
                    Err(err) => return Err(err.into()),
                };

                let request = match existing.restrict(request) {
                    Ok(_) => existing,
                    // FIXME: only match ValueError
                    Err(crate::Error::PyErr(err)) => {
                        return Ok(api::Compatibility::Incompatible(format!(
                            "conflicting requirement: {}",
                            err
                        )))
                    }
                    Err(err) => return Err(err),
                };

                let resolved = match state.get_current_resolve(request.pkg.name()) {
                    Ok(resolved) => resolved,
                    Err(errors::GetCurrentResolveError::PackageNotResolved(_)) => continue,
                };

                let compat = resolved.satisfies_pkg_request(&request);
                if !&compat {
                    return Ok(api::Compatibility::Incompatible(format!(
                        "conflicting requirement: '{}' {}",
                        request.pkg.name(),
                        compat
                    )));
                }
            }
        }

        Ok(api::Compatibility::Compatible)
    }
}

/// Validates that the var install requirements do not conflict with the existing options.
#[pyclass]
#[derive(Clone, Copy)]
pub struct VarRequirementsValidator {}

#[pymethods]
impl VarRequirementsValidator {
    #[new]
    fn new() -> Self {
        VarRequirementsValidator {}
    }

    #[pyo3(name = "validate")]
    fn validatepy(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        self.validate(state, spec)
    }
}

impl ValidatorT for VarRequirementsValidator {
    fn validate(
        &self,
        state: &graph::State,
        spec: &api::Spec,
    ) -> crate::Result<api::Compatibility> {
        if spec.pkg.is_source() {
            // source packages are not being "installed" so requests don't matter
            return Ok(api::Compatibility::Compatible);
        }

        let options = state.get_option_map();
        for request in &spec.install.requirements {
            if let api::Request::Var(request) = request {
                for (name, value) in options.iter() {
                    if *name != request.var
                        && !name.ends_with(&[".", request.var.as_str()].concat())
                    {
                        continue;
                    }
                    if value.is_empty() {
                        // empty option values do not provide a valuable opinion on the resolve
                        continue;
                    }
                    if request.value != *value {
                        return Ok(api::Compatibility::Incompatible(format!(
                            "package wants {}={}, resolve has {}={}",
                            request.var, request.value, name, value,
                        )));
                    }
                }
            }
        }
        Ok(api::Compatibility::Compatible)
    }
}

pub const fn default_validators() -> &'static [Validators] {
    &[
        Validators::Deprecation(DeprecationValidator {}),
        Validators::PackageRequest(PkgRequestValidator {}),
        Validators::Options(OptionsValidator {}),
        Validators::VarRequirements(VarRequirementsValidator {}),
        Validators::PkgRequirements(PkgRequirementsValidator {}),
        Validators::EmbeddedPackage(EmbeddedPackageValidator {}),
    ]
}
