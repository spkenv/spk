// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;

use crate::api::OptionMap;

use super::{
    graph::{self, Changes, Graph},
    solution::Solution,
    validation::{self, BinaryOnlyValidator, Validators},
};

#[pyclass]
pub struct Solver {
    repos: Vec<PyObject>,
    initial_state_builders: Vec<Changes>,
    validators: Vec<Validators>,
    _last_graph: Graph,
}

#[pymethods]
impl Solver {
    #[new]
    fn new() -> Self {
        Solver {
            repos: Vec::default(),
            initial_state_builders: Vec::default(),
            validators: validation::default_validators(),
            _last_graph: Graph {},
        }
    }

    pub fn reset(&mut self) {
        self.repos.clear();
        self.initial_state_builders.clear();
        self.validators = validation::default_validators();
    }

    /// If true, only solve pre-built binary packages.
    ///
    /// When false, the solver may return packages where the build is not set.
    /// These packages are known to have a source package available, and the requested
    /// options are valid for a new build of that source package.
    /// These packages are not actually built as part of the solver process but their
    /// build environments are fully resolved and dependencies included
    pub fn set_binary_only(&mut self, binary_only: bool) {
        let has_binary_only = self
            .validators
            .iter()
            .find_map(|v| match v {
                Validators::BinaryOnly(_) => Some(true),
                _ => None,
            })
            .unwrap_or(false);
        if !(has_binary_only ^ binary_only) {
            return;
        }
        if binary_only {
            // Add BinaryOnly validator because it was missing.
            self.validators
                .insert(0, Validators::BinaryOnly(BinaryOnlyValidator {}))
        } else {
            // Remove all BinaryOnly validators because one was found.
            self.validators = self
                .validators
                .iter()
                .filter(|v| !matches!(v, Validators::BinaryOnly(_)))
                .copied()
                .collect();
        }
    }

    pub fn solve(&self) -> Solution {
        todo!()
    }

    pub fn update_options(&mut self, options: OptionMap) {
        self.initial_state_builders
            .push(Changes::SetOptions(graph::SetOptions::new(options)))
    }
}
