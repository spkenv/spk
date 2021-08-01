// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{create_exception, prelude::*};
use std::sync::{Arc, RwLock};

use crate::api::OptionMap;

use super::{
    errors::SolverError,
    graph::{self, Changes, Decision, Graph, Node, State, DEAD_STATE},
    solution::Solution,
    validation::{self, BinaryOnlyValidator, Validators},
};

create_exception!(errors, SolverFailedError, SolverError);

#[pyclass]
pub struct Solver {
    repos: Vec<PyObject>,
    initial_state_builders: Vec<Changes>,
    validators: Vec<Validators>,
    last_graph: Arc<RwLock<Graph>>,
}

#[pymethods]
impl Solver {
    #[new]
    fn new() -> Self {
        Solver {
            repos: Vec::default(),
            initial_state_builders: Vec::default(),
            validators: validation::default_validators(),
            last_graph: Arc::new(RwLock::new(Graph::new())),
        }
    }

    pub fn get_initial_state(&self) -> State {
        todo!()
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

    pub fn solve(&mut self) -> PyResult<Solution> {
        let solve_graph = Arc::new(RwLock::new(Graph::new()));
        self.last_graph = solve_graph.clone();

        let mut history = Vec::new();
        let mut current_node: Option<Arc<RwLock<Node>>> = None;
        let mut decision = Some(Decision::new(self.initial_state_builders.clone()));

        while decision.is_some()
            && (current_node.is_none()
                || !current_node
                    .as_ref()
                    .map(|n| std::ptr::eq(&n.read().unwrap().state, &*DEAD_STATE))
                    .unwrap_or_default())
        {
            // The python code would `yield (current_node, decision)` here,
            // capturing the yielded value into SolverRuntime.solution.

            current_node = Some({
                let mut sg = solve_graph.write().unwrap();
                let root_id = sg.root.read().unwrap().id();
                sg.add_branch(
                    current_node
                        .map(|n| n.read().unwrap().id())
                        .unwrap_or(root_id),
                    &decision.unwrap(),
                )
            });
            decision = current_node
                .as_ref()
                .map(|n| self.step_state(&n.read().unwrap()))
                .flatten();
            history.push(current_node.clone());
        }

        let current_node = current_node.expect("current_node always `is_some` here");
        let current_node_lock = current_node.read().unwrap();

        let is_dead = current_node_lock.state.id()
            == solve_graph.read().unwrap().root.read().unwrap().state.id()
            || std::ptr::eq(&current_node_lock.state, &*DEAD_STATE);
        let is_empty = self.get_initial_state().get_pkg_requests().is_empty();
        if is_dead && !is_empty {
            Err(SolverFailedError::new_err(
                (*solve_graph).read().unwrap().clone(),
            ))
        } else {
            Ok(current_node_lock.state.as_solution())
        }
    }

    fn step_state(&self, _node: &Node) -> Option<Decision> {
        todo!()
    }

    pub fn update_options(&mut self, options: OptionMap) {
        self.initial_state_builders
            .push(Changes::SetOptions(graph::SetOptions::new(options)))
    }
}
