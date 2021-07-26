// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use std::{collections::HashMap, sync::Arc};

use crate::api;

use super::{
    package_iterator::PackageIterator,
    solution::{PackageSource, Solution},
};

lazy_static! {
    pub static ref DEAD_STATE: State = State {
        options: Vec::default(),
        packages: Vec::default(),
        pkg_requests: Vec::default(),
        var_requests: Vec::default(),
    };
}

#[derive(Clone)]
pub enum Changes {
    SetOptions(SetOptions),
}

pub trait ChangeT {}

/// A single change made to a state.
#[pyclass(subclass)]
pub struct Change {}

/// The decision represents a choice made by the solver.
///
/// Each decision connects one state to another in the graph.
#[pyclass]
#[derive(Clone)]
pub struct Decision {
    pub changes: Vec<Changes>,
    pub notes: Vec<Note>,
}

impl Decision {
    pub fn new(changes: Vec<Changes>) -> Self {
        Self {
            changes,
            notes: Vec::default(),
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Graph {
    pub root: Arc<Node>,
    pub nodes: HashMap<usize, Arc<Node>>,
}

impl Graph {
    pub fn new() -> Self {
        let dead_state = Arc::new(Node::new(DEAD_STATE.clone()));
        let dead_state_id = dead_state.id();
        let nodes = [(dead_state_id, dead_state.clone())]
            .iter()
            .cloned()
            .collect();
        Graph {
            root: dead_state,
            nodes,
        }
    }

    pub fn add_branch(&mut self, _source_id: usize, _decision: &Decision) -> Arc<Node> {
        todo!()
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Node {
    pub inputs: HashMap<usize, Decision>,
    pub outputs: HashMap<usize, Decision>,
    pub state: State,
    pub iterators: HashMap<String, PackageIterator>,
}

impl Node {
    fn new(state: State) -> Self {
        Node {
            inputs: HashMap::default(),
            outputs: HashMap::default(),
            state,
            iterators: HashMap::default(),
        }
    }

    #[inline]
    pub fn id(&self) -> usize {
        self.state.id()
    }
}

#[pyclass(subclass)]
#[derive(Clone)]
pub struct Note {}

#[pyclass(extends=Change)]
pub struct RequestPackage {}

#[pyclass(extends=Change)]
pub struct RequestVar {}

#[pyclass(extends=Change, subclass)]
#[derive(Clone)]
pub struct SetOptions {
    _options: api::OptionMap,
}

impl SetOptions {
    pub fn new(options: api::OptionMap) -> Self {
        SetOptions { _options: options }
    }
}

impl ChangeT for SetOptions {}

#[pyclass(extends=Change, subclass)]
pub struct SetPackage {}

#[pyclass(extends=SetPackage)]
pub struct SetPackageBuild {}

#[pyclass]
#[derive(Clone)]
pub struct State {
    pub pkg_requests: Vec<api::PkgRequest>,
    pub var_requests: Vec<api::VarRequest>,
    pub packages: Vec<(api::Spec, PackageSource)>,
    pub options: Vec<(String, String)>,
}

impl State {
    #[inline]
    pub fn id(&self) -> usize {
        todo!()
    }

    pub fn as_solution(&self) -> Solution {
        todo!()
    }
}

#[pyclass(extends=Note)]
pub struct SkipPackageNote {}

#[pyclass(extends=Change)]
pub struct StepBack {}
