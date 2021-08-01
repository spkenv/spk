// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::RwLock;
use std::{collections::HashMap, sync::Arc};

use crate::api::{self, Ident, InclusionPolicy};

use super::{
    package_iterator::PackageIterator,
    solution::{PackageSource, Solution},
};

lazy_static! {
    pub static ref DEAD_STATE: State = State::default();
}

const BRANCH_ALREADY_ATTEMPTED: &str = "Branch already attempted";

pub enum GraphError {
    RecursionError(&'static str),
}

pub type Result<T> = std::result::Result<T, GraphError>;

#[derive(Clone)]
pub enum Changes {
    SetOptions(SetOptions),
    StepBack(StepBack),
}

impl Changes {
    pub fn apply(&self, base: &State) -> State {
        match self {
            Changes::SetOptions(so) => so.apply(base),
            Changes::StepBack(sb) => sb.apply(base),
        }
    }
}

pub trait ChangeBaseT {
    fn as_decision(&self) -> Decision;
}

pub trait ChangeT {
    fn apply(&self, base: &State) -> State;
}

impl ChangeBaseT for Changes {
    fn as_decision(&self) -> Decision {
        Decision {
            changes: vec![self.clone()],
            notes: Vec::default(),
        }
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
    pub notes: Vec<NoteEnum>,
}

impl Decision {
    pub fn new(changes: Vec<Changes>) -> Self {
        Self {
            changes,
            notes: Vec::default(),
        }
    }

    pub fn add_notes<'a>(&mut self, notes: impl Iterator<Item = &'a NoteEnum>) {
        self.notes.extend(notes.cloned())
    }

    pub fn apply(&self, base: State) -> State {
        let mut state = base;
        for change in self.changes.iter() {
            state = change.apply(&state);
        }
        state
    }

    pub fn build_package(
        _spec: &api::Spec,
        _source: &PackageSource,
        _build_env: &Solution,
    ) -> Decision {
        todo!()
    }

    pub fn resolve_package(_spec: &api::Spec, _source: &PackageSource) -> Decision {
        todo!()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Graph {
    pub root: Arc<RwLock<Node>>,
    pub nodes: HashMap<u64, Arc<RwLock<Node>>>,
}

impl Graph {
    pub fn new() -> Self {
        let dead_state = Arc::new(RwLock::new(Node::new(DEAD_STATE.clone())));
        let dead_state_id = dead_state.read().unwrap().id();
        let nodes = [(dead_state_id, dead_state.clone())]
            .iter()
            .cloned()
            .collect();
        Graph {
            root: dead_state,
            nodes,
        }
    }

    pub fn add_branch(&mut self, source_id: u64, decision: &Decision) -> Result<Arc<RwLock<Node>>> {
        let old_node = self
            .nodes
            .get_mut(&source_id)
            .expect("source_id exists in nodes")
            .clone();
        let new_state = decision.apply(old_node.read().unwrap().state.clone());
        let mut new_node = Arc::new(RwLock::new(Node::new(new_state)));
        {
            let mut new_node_lock = new_node.write().unwrap();

            match self.nodes.get(&new_node_lock.id()) {
                None => {
                    self.nodes.insert(new_node_lock.id(), new_node.clone());
                    for (name, iterator) in old_node.read().unwrap().iterators.iter() {
                        new_node_lock.set_iterator(name, iterator.clone())
                    }
                }
                Some(node) => {
                    drop(new_node_lock);
                    new_node = node.clone();
                }
            }
        }

        let mut old_node_lock = old_node.write().unwrap();
        {
            // Avoid deadlock if old_node is the same node as new_node
            if !Arc::ptr_eq(&old_node, &new_node) {
                let mut new_node_lock = new_node.write().unwrap();
                old_node_lock.add_output(decision, &new_node_lock.state)?;
                new_node_lock.add_input(&old_node_lock.state, decision);
            } else {
                let old_state = old_node_lock.state.clone();
                old_node_lock.add_output(decision, &old_state)?;
                old_node_lock.add_input(&old_state, decision);
            }
        }
        Ok(new_node)
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
    pub fn add_input(&mut self, state: &State, decision: &Decision) {
        self.inputs.insert(state.id(), decision.clone());
    }

    pub fn add_output(&mut self, decision: &Decision, state: &State) -> Result<()> {
        if self.outputs.contains_key(&state.id()) {
            return Err(GraphError::RecursionError(BRANCH_ALREADY_ATTEMPTED));
        }
        self.outputs.insert(state.id(), decision.clone());
        Ok(())
    }

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

// XXX this should be called "Note", resolve python interop
#[derive(Clone, Debug)]
pub enum NoteEnum {
    SkipPackageNote(SkipPackageNote),
}

#[pyclass(extends=Change)]
pub struct RequestPackage {}

#[pyclass(extends=Change)]
pub struct RequestVar {}

#[pyclass(extends=Change, subclass)]
#[derive(Clone)]
pub struct SetOptions {
    options: api::OptionMap,
}

impl SetOptions {
    pub fn new(options: api::OptionMap) -> Self {
        SetOptions { options }
    }
}

impl ChangeT for SetOptions {
    fn apply(&self, base: &State) -> State {
        let mut options: HashMap<String, String> = base.options.iter().cloned().collect();
        for (k, v) in self.options.iter() {
            if v.is_empty() && options.contains_key(k) {
                continue;
            }
            options.insert(k.to_owned(), v.to_owned());
        }
        base.with_options(options.into_iter().collect())
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
        let mut solution = Solution::new(Some(self.options.iter().cloned().collect()));
        for (spec, source) in self.packages.iter() {
            let req = self.get_merged_request(spec.pkg.name());
            solution.add(&req, spec, source);
        }
        solution
    }

    fn get_merged_request(&self, _name: &str) -> api::PkgRequest {
        todo!()
    }

    pub fn get_next_request(&self) -> Option<api::PkgRequest> {
        // tests reveal this method is not safe to cache.
        let packages: HashSet<&str> = self
            .packages
            .iter()
            .map(|(spec, _)| spec.pkg.name())
            .collect();
        for request in self.pkg_requests.iter() {
            if packages.contains(request.pkg.name()) {
                continue;
            }
            if request.inclusion_policy == InclusionPolicy::IfAlreadyPresent {
                continue;
            }
            return Some(self.get_merged_request(request.pkg.name()));
        }

        None
    }

    pub fn get_option_map(&self) -> api::OptionMap {
        // TODO: cache this
        self.options.iter().cloned().collect()
    }

    pub fn get_pkg_requests(&self) -> &Vec<api::PkgRequest> {
        &self.pkg_requests
    }

    fn with_options(&self, options: Vec<(String, String)>) -> Self {
        State::new(
            self.pkg_requests.clone(),
            self.var_requests.clone(),
            self.packages.clone(),
            options,
        )
    }
}

#[derive(Clone, Debug)]
enum SkipPackageNoteReason {
    String(String),
    Compatibility(api::Compatibility),
}

#[pyclass(extends=Note)]
#[derive(Clone, Debug)]
pub struct SkipPackageNote {
    pkg: Ident,
    reason: SkipPackageNoteReason,
}

impl SkipPackageNote {
    pub fn new(pkg: Ident, reason: api::Compatibility) -> Self {
        SkipPackageNote {
            pkg,
            reason: SkipPackageNoteReason::Compatibility(reason),
        }
    }

    pub fn new_from_message(pkg: Ident, reason: &str) -> Self {
        SkipPackageNote {
            pkg,
            reason: SkipPackageNoteReason::String(reason.to_owned()),
        }
    }
}

#[pyclass(extends=Change)]
#[derive(Clone)]
pub struct StepBack {
    cause: String,
    destination: State,
}

impl StepBack {
    pub fn new(cause: &str, to: &State) -> Self {
        StepBack {
            cause: cause.to_owned(),
            destination: to.clone(),
        }
    }
}

impl ChangeT for StepBack {
    fn apply(&self, _base: &State) -> State {
        self.destination.clone()
    }
}
