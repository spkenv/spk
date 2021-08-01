// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::{collections::HashMap, sync::Arc};

use crate::api;

use super::{
    package_iterator::PackageIterator,
    solution::{PackageSource, Solution},
};

lazy_static! {
    pub static ref DEAD_STATE: State = State::default();
}

#[derive(Clone)]
pub enum Changes {
    SetOptions(SetOptions),
}

pub trait ChangeT {
    fn apply(&self, base: &State) -> State;
}

impl ChangeT for Changes {
    fn apply(&self, _base: &State) -> State {
        todo!()
    }
}

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

    pub fn apply(&self, base: State) -> State {
        let mut state = base;
        for change in self.changes.iter() {
            state = change.apply(&state);
        }
        state
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Graph {
    pub root: Arc<Node>,
    pub nodes: HashMap<u64, Arc<Node>>,
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

    pub fn add_branch(&mut self, _source_id: u64, _decision: &Decision) -> Arc<Node> {
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
    pub inputs: HashMap<u64, Decision>,
    pub outputs: HashMap<u64, Decision>,
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
    pub fn id(&self) -> u64 {
        self.state.id()
    }

    pub fn set_iterator(&mut self, package_name: &str, iterator: PackageIterator) {
        if self.iterators.contains_key(package_name) {
            panic!("iterator already exists [INTERNAL ERROR]");
        }
        self.iterators.insert(package_name.to_owned(), iterator);
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

#[pyclass(extends=Change, subclass)]
pub struct SetPackage {}

#[pyclass(extends=SetPackage)]
pub struct SetPackageBuild {}

#[pyclass]
#[derive(Clone)]
pub struct State {
    pkg_requests: Vec<api::PkgRequest>,
    var_requests: Vec<api::VarRequest>,
    packages: Vec<(api::Spec, PackageSource)>,
    options: Vec<(String, String)>,
    hash_cache: u64,
}

impl Default for State {
    fn default() -> Self {
        State::new(
            Vec::default(),
            Vec::default(),
            Vec::default(),
            Vec::default(),
        )
    }
}

impl State {
    pub fn new(
        pkg_requests: Vec<api::PkgRequest>,
        var_requests: Vec<api::VarRequest>,
        packages: Vec<(api::Spec, PackageSource)>,
        options: Vec<(String, String)>,
    ) -> Self {
        // TODO: This pre-calculates the hash but there
        // may be states constructed where the id is
        // never accessed. Determine if it is better
        // to lazily compute this on demand.
        //
        // TODO: Since new states are constructed from
        // old states by modifying one field at a time,
        // it would be more efficient to save the hash
        // of each of the four members, so those individual
        // hashes don't need to be recalculated in the
        // new object.
        let mut hasher = DefaultHasher::new();
        pkg_requests.hash(&mut hasher);
        var_requests.hash(&mut hasher);
        for (p, _) in packages.iter() {
            p.hash(&mut hasher)
        }
        options.hash(&mut hasher);

        State {
            pkg_requests,
            var_requests,
            packages,
            options,
            hash_cache: hasher.finish(),
        }
    }

    #[inline]
    pub fn id(&self) -> u64 {
        self.hash_cache
    }

    pub fn as_solution(&self) -> Solution {
        todo!()
    }

    pub fn get_pkg_requests(&self) -> &Vec<api::PkgRequest> {
        &self.pkg_requests
    }
}

#[pyclass(extends=Note)]
pub struct SkipPackageNote {}

#[pyclass(extends=Change)]
pub struct StepBack {}
