// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;

use crate::api;

use super::graph;

#[derive(Clone, Copy)]
pub enum Validators {
    Deprecation(DeprecationValidator),
    BinaryOnly(BinaryOnlyValidator),
}

pub trait ValidatorT {
    /// Check if the given package is appropriate for the provided state.
    fn validate(&self, state: &graph::State, spec: &api::Spec) -> api::Compatibility;
}

impl ValidatorT for Validators {
    fn validate(&self, state: &graph::State, spec: &api::Spec) -> api::Compatibility {
        match self {
            Validators::Deprecation(v) => v.validate(state, spec),
            Validators::BinaryOnly(v) => v.validate(state, spec),
        }
    }
}

#[pyclass(subclass)]
pub struct Validator {}

/// Ensures that deprecated packages are not included unless specifically requested.
#[pyclass(extends=Validator)]
#[derive(Clone, Copy)]
pub struct DeprecationValidator {}

impl ValidatorT for DeprecationValidator {
    fn validate(&self, _state: &graph::State, _spec: &api::Spec) -> api::Compatibility {
        todo!()
    }
}

/// Enforces the resolution of binary packages only, denying new builds from source.
#[pyclass(extends=Validator)]
#[derive(Clone, Copy)]
pub struct BinaryOnlyValidator {}

impl ValidatorT for BinaryOnlyValidator {
    fn validate(&self, _state: &graph::State, _spec: &api::Spec) -> api::Compatibility {
        todo!()
    }
}

pub fn default_validators() -> Vec<Validators> {
    vec![Validators::Deprecation(DeprecationValidator {})]
}
