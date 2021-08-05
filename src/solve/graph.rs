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

use super::errors;
use super::{
    package_iterator::PackageIterator,
    solution::{PackageSource, Solution},
};

lazy_static! {
    pub static ref DEAD_STATE: Arc<State> = Arc::new(State::default());
}

const BRANCH_ALREADY_ATTEMPTED: &str = "Branch already attempted";

#[derive(Debug)]
pub enum GraphError {
    RecursionError(&'static str),
}

pub type Result<T> = std::result::Result<T, GraphError>;

#[derive(Clone, Debug)]
pub enum Changes {
    RequestPackage(RequestPackage),
    RequestVar(RequestVar),
    SetOptions(SetOptions),
    SetPackage(Box<SetPackage>),
    SetPackageBuild(Box<SetPackageBuild>),
    StepBack(StepBack),
}

impl Changes {
    pub fn apply(&self, base: &State) -> State {
        match self {
            Changes::RequestPackage(rp) => rp.apply(base),
            Changes::RequestVar(rv) => rv.apply(base),
            Changes::SetOptions(so) => so.apply(base),
            Changes::SetPackage(sp) => sp.apply(base),
            Changes::SetPackageBuild(spb) => spb.apply(base),
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
#[derive(Clone)]
pub struct Change {}

/// The decision represents a choice made by the solver.
///
/// Each decision connects one state to another in the graph.
#[pyclass]
#[derive(Clone, Debug)]
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
        spec: api::Spec,
        _source: &PackageSource,
        build_env: &Solution,
    ) -> crate::Result<Decision> {
        let self_spec = spec;

        let generate_changes = || -> crate::Result<Vec<_>> {
            let mut changes = Vec::<Changes>::new();

            let specs = build_env.items().map(|(_, s, _)| s).collect::<Vec<_>>();
            let options = build_env.options();
            let mut spec = self_spec.clone();
            spec.update_spec_for_build(&options, specs)?;

            changes.push(Changes::SetPackageBuild(Box::new(SetPackageBuild::new(
                spec.clone(),
                self_spec.clone(),
            ))));
            for req in &spec.install.requirements {
                match req {
                    api::Request::Pkg(req) => {
                        changes.push(Changes::RequestPackage(RequestPackage::new(req.clone())))
                    }
                    api::Request::Var(req) => {
                        changes.push(Changes::RequestVar(RequestVar::new(req.clone())))
                    }
                }
            }

            let mut opts = api::OptionMap::default();
            opts.insert(
                self_spec.pkg.name().to_owned(),
                self_spec.compat.render(&self_spec.pkg.version),
            );
            for opt in &spec.build.options {
                let value = opt.get_value(None);
                if !value.is_empty() {
                    let name = opt.namespaced_name(spec.pkg.name());
                    opts.insert(name, value);
                }
            }
            if !opts.is_empty() {
                changes.push(Changes::SetOptions(SetOptions::new(opts)));
            }

            Ok(changes)
        };

        Ok(Decision {
            changes: generate_changes()?,
            notes: Vec::default(),
        })
    }

    pub fn resolve_package(spec: &api::Spec, source: PackageSource) -> Decision {
        let generate_changes = || {
            let mut changes = vec![Changes::SetPackage(Box::new(SetPackage::new(
                spec.clone(),
                source,
            )))];

            // installation options are not relevant for source packages
            if spec.pkg.is_source() {
                return changes;
            }

            for req in &spec.install.requirements {
                match req {
                    api::Request::Pkg(req) => {
                        changes.push(Changes::RequestPackage(RequestPackage::new(req.clone())))
                    }
                    api::Request::Var(req) => {
                        changes.push(Changes::RequestVar(RequestVar::new(req.clone())))
                    }
                }
            }

            for embedded in &spec.install.embedded {
                changes.push(Changes::RequestPackage(RequestPackage::new(
                    api::PkgRequest::from_ident(&embedded.pkg),
                )));
                changes.push(Changes::SetPackage(Box::new(SetPackage::new(
                    embedded.clone(),
                    PackageSource::Spec(Box::new(spec.clone())),
                ))));
            }

            let mut opts = api::OptionMap::default();
            opts.insert(
                spec.pkg.name().to_owned(),
                spec.compat.render(&spec.pkg.version),
            );
            for opt in &spec.build.options {
                let value = opt.get_value(None);
                if !value.is_empty() {
                    let name = opt.namespaced_name(spec.pkg.name());
                    opts.insert(name, value);
                }
            }
            if !opts.is_empty() {
                changes.push(Changes::SetOptions(SetOptions::new(opts)));
            }

            changes
        };

        Decision {
            changes: generate_changes(),
            notes: Vec::default(),
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
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
        let new_state = decision.apply((*old_node.read().unwrap().state).clone());
        let mut new_node = Arc::new(RwLock::new(Node::new(Arc::new(new_state))));
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
#[derive(Clone, Debug)]
pub struct Node {
    pub inputs: HashMap<u64, Decision>,
    pub outputs: HashMap<u64, Decision>,
    pub state: Arc<State>,
    pub iterators: HashMap<String, Box<dyn PackageIterator>>,
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

    pub fn get_iterator(&self, package_name: &str) -> Option<Box<dyn PackageIterator>> {
        self.iterators.get(package_name).cloned()
    }

    fn new(state: Arc<State>) -> Self {
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

    pub fn set_iterator(&mut self, package_name: &str, iterator: Box<dyn PackageIterator>) {
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
#[derive(Clone, Debug)]
pub struct RequestPackage {
    request: api::PkgRequest,
}

impl RequestPackage {
    pub fn new(request: api::PkgRequest) -> Self {
        RequestPackage { request }
    }
}

impl ChangeT for RequestPackage {
    fn apply(&self, base: &State) -> State {
        // XXX: An immutable data structure for pkg_requests would
        // allow for sharing.
        let mut new_requests = base.pkg_requests.clone();
        new_requests.push(self.request.clone());
        base.with_pkg_requests(new_requests)
    }
}

#[pyclass(extends=Change)]
#[derive(Clone, Debug)]
pub struct RequestVar {
    request: api::VarRequest,
}

impl RequestVar {
    pub fn new(request: api::VarRequest) -> Self {
        RequestVar { request }
    }
}

impl ChangeT for RequestVar {
    fn apply(&self, base: &State) -> State {
        // XXX: An immutable data structure for var_requests would
        // allow for sharing.
        let mut new_requests = base.var_requests.clone();
        new_requests.push(self.request.clone());
        let mut options = base
            .options
            .iter()
            .cloned()
            .filter(|(var, _)| *var != self.request.var)
            .collect::<Vec<_>>();
        options.push((self.request.var.to_owned(), self.request.value.to_owned()));
        base.with_var_requests_and_options(new_requests, options)
    }
}

#[pyclass(extends=Change, subclass)]
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct SetPackage {
    spec: api::Spec,
    source: PackageSource,
}

impl SetPackage {
    fn new(spec: api::Spec, source: PackageSource) -> Self {
        SetPackage { spec, source }
    }
}

impl ChangeT for SetPackage {
    fn apply(&self, base: &State) -> State {
        base.with_package(self.spec.clone(), self.source.clone())
    }
}

/// Sets a package in the resolve, denoting is as a new build.
#[pyclass(extends=SetPackage)]
#[derive(Clone, Debug)]
pub struct SetPackageBuild {
    spec: api::Spec,
    source: PackageSource,
}

impl SetPackageBuild {
    fn new(spec: api::Spec, source: api::Spec) -> Self {
        SetPackageBuild {
            spec,
            source: PackageSource::Spec(Box::new(source)),
        }
    }
}

impl ChangeT for SetPackageBuild {
    fn apply(&self, base: &State) -> State {
        base.with_package(self.spec.clone(), self.source.clone())
    }
}

#[pyclass]
#[derive(Clone, Debug)]
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

    pub fn as_solution(&self) -> PyResult<Solution> {
        let mut solution = Solution::new(Some(self.options.iter().cloned().collect()));
        for (spec, source) in self.packages.iter() {
            let req = self.get_merged_request(spec.pkg.name())?;
            solution.add(&req, spec, source.clone());
        }
        Ok(solution)
    }

    pub fn get_current_resolve(&self, name: &str) -> errors::GetCurrentResolveResult<api::Spec> {
        // TODO: cache this
        for (spec, _) in &self.packages {
            if spec.pkg.name() == name {
                return Ok(spec.clone());
            }
        }
        Err(errors::GetCurrentResolveError::PackageNotResolved(format!(
            "Has not been resolved: '{}'",
            name
        )))
    }

    pub fn get_merged_request(
        &self,
        name: &str,
    ) -> errors::GetMergedRequestResult<api::PkgRequest> {
        // tests reveal this method is not safe to cache.
        let mut merged: Option<api::PkgRequest> = None;
        for request in self.pkg_requests.iter() {
            match merged.as_mut() {
                None => {
                    if request.pkg.name() != name {
                        continue;
                    }
                    merged = Some(request.clone());
                }
                Some(merged) => {
                    if request.pkg.name() != merged.pkg.name() {
                        continue;
                    }
                    merged.restrict(request)?;
                }
            }
        }
        match merged {
            Some(merged) => Ok(merged),
            None => Err(errors::GetMergedRequestError::NoRequestFor(format!(
                "No requests for '{}' [INTERNAL ERROR]",
                name
            ))),
        }
    }

    pub fn get_next_request(&self) -> PyResult<Option<api::PkgRequest>> {
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
            return Ok(Some(self.get_merged_request(request.pkg.name())?));
        }

        Ok(None)
    }

    pub fn get_option_map(&self) -> api::OptionMap {
        // TODO: cache this
        self.options.iter().cloned().collect()
    }

    pub fn get_pkg_requests(&self) -> &Vec<api::PkgRequest> {
        &self.pkg_requests
    }

    pub fn get_var_requests(&self) -> &Vec<api::VarRequest> {
        &self.var_requests
    }

    fn with_options(&self, options: Vec<(String, String)>) -> Self {
        State::new(
            self.pkg_requests.clone(),
            self.var_requests.clone(),
            self.packages.clone(),
            options,
        )
    }

    fn with_package(&self, spec: api::Spec, source: PackageSource) -> Self {
        let mut new_packages = self.packages.clone();
        new_packages.push((spec, source));

        State::new(
            self.pkg_requests.clone(),
            self.var_requests.clone(),
            new_packages,
            self.options.clone(),
        )
    }

    fn with_pkg_requests(&self, pkg_requests: Vec<api::PkgRequest>) -> Self {
        State::new(
            pkg_requests,
            self.var_requests.clone(),
            self.packages.clone(),
            self.options.clone(),
        )
    }

    fn with_var_requests_and_options(
        &self,
        var_requests: Vec<api::VarRequest>,
        options: Vec<(String, String)>,
    ) -> Self {
        State::new(
            self.pkg_requests.clone(),
            var_requests,
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
#[derive(Clone, Debug)]
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
