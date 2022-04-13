// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use once_cell::sync::Lazy;
use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::iter::FromIterator;
use std::sync::{Mutex, RwLock};
use std::{collections::HashMap, sync::Arc};

use crate::api::{self, Ident, InclusionPolicy};

use super::errors::{self, GetMergedRequestError};
use super::{
    package_iterator::PackageIterator,
    solution::{PackageSource, Solution},
};

#[cfg(test)]
#[path = "./graph_test.rs"]
mod graph_test;

pub static DEAD_STATE: Lazy<Arc<State>> = Lazy::new(|| Arc::new(State::default()));

const BRANCH_ALREADY_ATTEMPTED: &str = "Branch already attempted";

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum GraphError {
    RecursionError(&'static str),
    RequestError(errors::GetMergedRequestError),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RecursionError(s) => s.fmt(f),
            Self::RequestError(s) => s.fmt(f),
        }
    }
}

impl From<GetMergedRequestError> for GraphError {
    fn from(err: GetMergedRequestError) -> Self {
        Self::RequestError(err)
    }
}

pub type Result<T> = std::result::Result<T, GraphError>;

#[derive(Clone, Debug)]
pub enum Change {
    RequestPackage(RequestPackage),
    RequestVar(RequestVar),
    SetOptions(SetOptions),
    SetPackage(Box<SetPackage>),
    SetPackageBuild(Box<SetPackageBuild>),
    StepBack(StepBack),
}

impl Change {
    pub fn apply(&self, base: &State) -> Arc<State> {
        match self {
            Change::RequestPackage(rp) => rp.apply(base),
            Change::RequestVar(rv) => rv.apply(base),
            Change::SetOptions(so) => so.apply(base),
            Change::SetPackage(sp) => sp.apply(base),
            Change::SetPackageBuild(spb) => spb.apply(base),
            Change::StepBack(sb) => sb.apply(base),
        }
    }
}

impl Change {
    pub fn as_decision(&self) -> Decision {
        Decision {
            changes: vec![self.clone()],
            notes: Vec::default(),
        }
    }
}

/// The decision represents a choice made by the solver.
///
/// Each decision connects one state to another in the graph.
#[derive(Clone, Debug)]
pub struct Decision {
    pub changes: Vec<Change>,
    pub notes: Vec<Note>,
}

impl Decision {
    pub fn builder<'state>(
        spec: Arc<api::Spec>,
        base: &'state State,
    ) -> DecisionBuilder<'state, 'static> {
        DecisionBuilder::new(spec, base)
    }

    pub fn new(changes: Vec<Change>) -> Self {
        Self {
            changes,
            notes: Vec::default(),
        }
    }

    pub fn apply(&self, base: &Arc<State>) -> Arc<State> {
        let mut state = None;
        let else_closure = || Arc::clone(base);
        for change in self.changes.iter() {
            state = Some(change.apply(&state.unwrap_or_else(else_closure)));
        }
        state.unwrap_or_else(else_closure)
    }

    pub fn add_notes(&mut self, notes: impl IntoIterator<Item = Note>) {
        self.notes.extend(notes)
    }
}

pub struct DecisionBuilder<'state, 'cmpt> {
    base: &'state State,
    spec: Arc<api::Spec>,
    components: HashSet<&'cmpt api::Component>,
}

impl<'state> DecisionBuilder<'state, 'static> {
    pub fn new(spec: Arc<api::Spec>, base: &'state State) -> Self {
        Self {
            base,
            spec,
            components: HashSet::new(),
        }
    }
}

impl<'state, 'cmpt> DecisionBuilder<'state, 'cmpt> {
    pub fn with_components<'a>(
        self,
        components: impl IntoIterator<Item = &'a api::Component>,
    ) -> DecisionBuilder<'state, 'a> {
        DecisionBuilder {
            components: components.into_iter().collect(),
            spec: self.spec,
            base: self.base,
        }
    }

    pub fn build_package(self, build_env: &Solution) -> crate::Result<Decision> {
        let generate_changes = || -> crate::Result<Vec<_>> {
            let mut changes = Vec::<Change>::new();

            let specs = build_env.items().into_iter().map(|s| s.spec);
            let options = build_env.options();
            let mut spec = (*self.spec).clone();
            spec.update_for_build(&options, specs)?;
            let spec = Arc::new(spec);

            changes.push(Change::SetPackageBuild(Box::new(SetPackageBuild::new(
                spec.clone(),
                self.spec.clone(),
            ))));

            changes.extend(self.requirements_to_changes(&self.spec.install.requirements));
            changes.extend(self.components_to_changes(&self.spec.install.components));
            changes.extend(self.embedded_to_changes(&self.spec.install.embedded));
            changes.push(Self::options_to_change(&spec));

            Ok(changes)
        };

        Ok(Decision {
            changes: generate_changes()?,
            notes: Vec::default(),
        })
    }

    pub fn resolve_package(self, source: PackageSource) -> Decision {
        let generate_changes = || {
            let mut changes = vec![Change::SetPackage(Box::new(SetPackage::new(
                self.spec.clone(),
                source,
            )))];

            // installation options are not relevant for source packages
            if self.spec.pkg.is_source() {
                return changes;
            }

            changes.extend(self.requirements_to_changes(&self.spec.install.requirements));
            changes.extend(self.components_to_changes(&self.spec.install.components));
            changes.extend(self.embedded_to_changes(&self.spec.install.embedded));
            changes.push(Self::options_to_change(&self.spec));

            changes
        };

        Decision {
            changes: generate_changes(),
            notes: Vec::default(),
        }
    }

    fn requirements_to_changes(&self, requirements: &api::RequirementsList) -> Vec<Change> {
        requirements
            .iter()
            .flat_map(|req| match req {
                api::Request::Pkg(req) => self.pkg_request_to_changes(req),
                api::Request::Var(req) => vec![Change::RequestVar(RequestVar::new(req.clone()))],
            })
            .collect()
    }

    fn components_to_changes(&self, components: &api::ComponentSpecList) -> Vec<Change> {
        let mut changes = vec![];
        let required = self
            .spec
            .install
            .components
            .resolve_uses(self.components.iter().cloned());
        for component in components.iter() {
            if !required.contains(&component.name) {
                continue;
            }
            changes.extend(self.requirements_to_changes(&component.requirements));
            changes.extend(self.embedded_to_changes(&component.embedded));
        }
        changes
    }

    fn pkg_request_to_changes(&self, req: &api::PkgRequest) -> Vec<Change> {
        let mut req = std::borrow::Cow::Borrowed(req);
        if req.pkg.components.is_empty() {
            // if no component was requested specifically,
            // then we must assume the default run component
            req.to_mut().pkg.components.insert(api::Component::Run);
        }
        let mut changes = vec![Change::RequestPackage(RequestPackage::new(
            req.clone().into_owned(),
        ))];
        // we need to check if this request will change a previously
        // resolved package (eg: by adding a new component with new requirements)
        let (spec, _source) = match self.base.get_current_resolve(req.pkg.name()) {
            Ok(e) => e,
            Err(_) => return changes,
        };
        let existing = match self.base.get_merged_request(req.pkg.name()) {
            Ok(r) => r,
            // the error case here should not be possible since we have
            // already found a resolved package...
            Err(_) => return changes,
        };
        let new_components = spec
            .install
            .components
            .resolve_uses(req.pkg.components.iter());
        let existing_components = spec
            .install
            .components
            .resolve_uses(existing.pkg.components.iter());
        let added_components = new_components
            .difference(&existing_components)
            .collect::<HashSet<_>>();
        if added_components.is_empty() {
            return changes;
        }
        for component in spec.install.components.iter() {
            if !added_components.contains(&component.name) {
                continue;
            }
            changes.extend(self.requirements_to_changes(&component.requirements));
        }
        changes
    }

    fn embedded_to_changes(&self, embedded: &api::EmbeddedPackagesList) -> Vec<Change> {
        embedded
            .iter()
            .flat_map(|embedded| {
                [
                    Change::RequestPackage(RequestPackage::new(api::PkgRequest::from_ident(
                        &embedded.pkg,
                    ))),
                    Change::SetPackage(Box::new(SetPackage::new(
                        Arc::new(embedded.clone()),
                        PackageSource::Spec(self.spec.clone()),
                    ))),
                ]
            })
            .collect()
    }

    fn options_to_change(spec: &api::Spec) -> Change {
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
        Change::SetOptions(SetOptions::new(opts))
    }
}

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

    pub fn add_branch(&mut self, source_id: u64, decision: Decision) -> Result<Arc<RwLock<Node>>> {
        let old_node = self
            .nodes
            .get(&source_id)
            .expect("source_id exists in nodes")
            .clone();
        let new_state = decision.apply(&(old_node.read().unwrap().state));
        let mut new_node = Arc::new(RwLock::new(Node::new(new_state)));
        {
            let mut new_node_lock = new_node.write().unwrap();

            match self.nodes.get(&new_node_lock.id()) {
                None => {
                    self.nodes.insert(new_node_lock.id(), new_node.clone());
                    for (name, iterator) in old_node.read().unwrap().iterators.iter() {
                        new_node_lock.set_iterator(name, iterator)
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
                old_node_lock.add_output(decision.clone(), &new_node_lock.state)?;
                new_node_lock.add_input(&old_node_lock.state, decision);
            } else {
                let old_state = old_node_lock.state.clone();
                old_node_lock.add_output(decision.clone(), &old_state)?;
                old_node_lock.add_input(&old_state, decision);
            }
        }
        Ok(new_node)
    }

    pub fn walk(&self) -> GraphIter {
        GraphIter::new(self)
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

enum WalkState {
    ToProcessNonEmpty,
    YieldOuts,
    YieldNextNode(Decision),
}

pub struct GraphIter<'graph> {
    graph: &'graph Graph,
    node_outputs: HashMap<u64, VecDeque<Decision>>,
    to_process: VecDeque<Arc<RwLock<Node>>>,
    /// Which entry of node_outputs is currently being worked on.
    outs: Option<u64>,
    iter_node: Arc<RwLock<Node>>,
    walk_state: WalkState,
}

impl<'graph> GraphIter<'graph> {
    pub fn new(graph: &'graph Graph) -> GraphIter<'graph> {
        let to_process = VecDeque::from_iter([graph.root.clone()]);
        let iter_node = graph.root.clone();
        GraphIter {
            graph,
            node_outputs: HashMap::default(),
            to_process,
            outs: None,
            iter_node,
            walk_state: WalkState::ToProcessNonEmpty,
        }
    }
}

impl<'graph> Iterator for GraphIter<'graph> {
    type Item = (Node, Decision);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.walk_state {
                WalkState::ToProcessNonEmpty => {
                    if self.to_process.is_empty() {
                        self.walk_state = WalkState::YieldOuts;
                        continue;
                    }

                    let node = self.to_process.pop_front().unwrap();
                    let node_lock = node.read().unwrap();

                    if let Entry::Vacant(e) = self.node_outputs.entry(node_lock.id()) {
                        e.insert(node_lock.outputs_decisions.iter().cloned().collect());

                        for decision in node_lock.outputs_decisions.iter().rev() {
                            let destination = decision.apply(&node_lock.state);
                            self.to_process.push_front(
                                self.graph.nodes.get(&destination.id()).unwrap().clone(),
                            );
                        }
                    }

                    self.outs = Some(node_lock.id());
                    let outs = self.node_outputs.get_mut(&node_lock.id()).unwrap();
                    if outs.is_empty() {
                        continue;
                    }

                    self.to_process.push_back(node.clone());
                    let decision = outs.pop_front().unwrap();
                    return Some((node_lock.clone(), decision));
                }
                WalkState::YieldOuts => {
                    let outs = self.node_outputs.get_mut(&self.outs.unwrap()).unwrap();
                    if outs.is_empty() {
                        return None;
                    }

                    let node_lock = self.iter_node.read().unwrap();

                    let decision = outs.pop_front().unwrap();
                    self.walk_state = WalkState::YieldNextNode(decision.clone());
                    return Some((node_lock.clone(), decision));
                }
                WalkState::YieldNextNode(ref decision) => {
                    let next_state_id = {
                        let node_lock = self.iter_node.read().unwrap();

                        let next_state = decision.apply(&node_lock.state);
                        next_state.id()
                    };
                    self.iter_node = self.graph.nodes.get(&next_state_id).unwrap().clone();
                    self.walk_state = WalkState::YieldOuts;
                    continue;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    // Preserve order of inputs/outputs for iterating
    // in the same order.
    inputs: HashSet<u64>,
    inputs_decisions: Vec<Decision>,
    outputs: HashSet<u64>,
    outputs_decisions: Vec<Decision>,
    pub state: Arc<State>,
    iterators: HashMap<String, Arc<Mutex<Box<dyn PackageIterator>>>>,
}

impl Node {
    pub fn add_input(&mut self, state: &State, decision: Decision) {
        self.inputs.insert(state.id());
        self.inputs_decisions.push(decision);
    }

    pub fn add_output(&mut self, decision: Decision, state: &State) -> Result<()> {
        if self.outputs.contains(&state.id()) {
            return Err(GraphError::RecursionError(BRANCH_ALREADY_ATTEMPTED));
        }
        self.outputs.insert(state.id());
        self.outputs_decisions.push(decision);
        Ok(())
    }

    pub fn get_iterator(&self, package_name: &str) -> Option<Arc<Mutex<Box<dyn PackageIterator>>>> {
        self.iterators.get(package_name).cloned()
    }

    pub fn new(state: Arc<State>) -> Self {
        Node {
            inputs: HashSet::default(),
            inputs_decisions: Vec::default(),
            outputs: HashSet::default(),
            outputs_decisions: Vec::default(),
            state,
            iterators: HashMap::default(),
        }
    }

    #[inline]
    pub fn id(&self) -> u64 {
        self.state.id()
    }

    pub fn set_iterator(
        &mut self,
        package_name: &str,
        iterator: &Arc<Mutex<Box<dyn PackageIterator>>>,
    ) {
        if self.iterators.contains_key(package_name) {
            panic!("iterator already exists [INTERNAL ERROR]");
        }
        self.iterators.insert(
            package_name.to_owned(),
            Arc::new(Mutex::new(iterator.lock().unwrap().clone())),
        );
    }
}

/// Some additional information left by the solver
#[derive(Clone, Debug)]
pub enum Note {
    SkipPackageNote(SkipPackageNote),
    Other(String),
}

#[derive(Clone, Debug)]
pub struct RequestPackage {
    pub request: api::PkgRequest,
}

impl RequestPackage {
    pub fn new(request: api::PkgRequest) -> Self {
        RequestPackage { request }
    }

    pub fn apply(&self, base: &State) -> Arc<State> {
        // XXX: An immutable data structure for pkg_requests would
        // allow for sharing.
        let mut new_requests = (*base.pkg_requests).clone();
        new_requests.push(self.request.clone());
        Arc::new(base.with_pkg_requests(new_requests))
    }
}

#[derive(Clone, Debug)]
pub struct RequestVar {
    pub request: api::VarRequest,
}

impl RequestVar {
    pub fn new(request: api::VarRequest) -> Self {
        RequestVar { request }
    }

    pub fn apply(&self, base: &State) -> Arc<State> {
        // XXX: An immutable data structure for var_requests would
        // allow for sharing.
        let mut new_requests = (*base.var_requests).clone();
        new_requests.push(self.request.clone());
        let mut options = base
            .options
            .iter()
            .cloned()
            .filter(|(var, _)| *var != self.request.var)
            .collect::<Vec<_>>();
        options.push((self.request.var.to_owned(), self.request.value.to_owned()));
        Arc::new(base.with_var_requests_and_options(new_requests, options))
    }
}

#[derive(Clone, Debug)]
pub struct SetOptions {
    pub options: api::OptionMap,
}

impl SetOptions {
    pub fn new(options: api::OptionMap) -> Self {
        SetOptions { options }
    }

    pub fn apply(&self, base: &State) -> Arc<State> {
        // Update options while preserving order to match
        // python dictionary behaviour. "Updating a key
        // does not affect the order."
        let mut insertion_order = 0;
        // Build a lookup hash with an insertion order.
        let mut options: HashMap<String, (i32, String)> = base
            .options
            .iter()
            .cloned()
            .map(|(var, value)| {
                let i = insertion_order;
                insertion_order += 1;
                (var, (i, value))
            })
            .collect();
        // Update base options with request options...
        for (k, v) in self.options.iter() {
            match options.get_mut(k) {
                // Unless already present and request option value is empty.
                Some(_) if v.is_empty() => continue,
                // If option already existed, keep same insertion order.
                Some((_, value)) => *value = v.to_owned(),
                // New options are inserted at the end.
                None => {
                    let i = insertion_order;
                    insertion_order += 1;
                    options.insert(k.to_owned(), (i, v.to_owned()));
                }
            };
        }
        let mut options = options.into_iter().collect::<Vec<_>>();
        options.sort_by_key(|(_, (i, _))| *i);
        Arc::new(
            base.with_options(
                options
                    .into_iter()
                    .map(|(var, (_, value))| (var, value))
                    .collect::<Vec<_>>(),
            ),
        )
    }
}

#[derive(Clone, Debug)]
pub struct SetPackage {
    pub spec: Arc<api::Spec>,
    pub source: PackageSource,
}

impl SetPackage {
    pub fn new(spec: Arc<api::Spec>, source: PackageSource) -> Self {
        SetPackage { spec, source }
    }

    pub fn apply(&self, base: &State) -> Arc<State> {
        Arc::new(base.with_package(self.spec.clone(), self.source.clone()))
    }
}

/// Sets a package in the resolve, denoting is as a new build.
#[derive(Clone, Debug)]
pub struct SetPackageBuild {
    pub spec: Arc<api::Spec>,
    pub source: PackageSource,
}

impl SetPackageBuild {
    pub fn new(spec: Arc<api::Spec>, source: Arc<api::Spec>) -> Self {
        SetPackageBuild {
            spec,
            source: PackageSource::Spec(source),
        }
    }

    pub fn apply(&self, base: &State) -> Arc<State> {
        Arc::new(base.with_package(self.spec.clone(), self.source.clone()))
    }
}

#[derive(Clone, Debug)]
struct StateId {
    pkg_requests_hash: u64,
    var_requests_hash: u64,
    packages_hash: u64,
    options_hash: u64,
    full_hash: u64,
}

impl StateId {
    #[inline]
    fn id(&self) -> u64 {
        self.full_hash
    }

    pub fn new(
        pkg_requests_hash: u64,
        var_requests_hash: u64,
        packages_hash: u64,
        options_hash: u64,
    ) -> Self {
        let full_hash = {
            let mut hasher = DefaultHasher::new();
            pkg_requests_hash.hash(&mut hasher);
            var_requests_hash.hash(&mut hasher);
            packages_hash.hash(&mut hasher);
            options_hash.hash(&mut hasher);
            hasher.finish()
        };
        Self {
            pkg_requests_hash,
            var_requests_hash,
            packages_hash,
            options_hash,
            full_hash,
        }
    }

    fn options_hash(options: &[(String, String)]) -> u64 {
        let mut hasher = DefaultHasher::new();
        options.hash(&mut hasher);
        hasher.finish()
    }

    fn pkg_requests_hash(pkg_requests: &[api::PkgRequest]) -> u64 {
        let mut hasher = DefaultHasher::new();
        pkg_requests.hash(&mut hasher);
        hasher.finish()
    }

    fn packages_hash(packages: &[(Arc<api::Spec>, PackageSource)]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for (p, _) in packages.iter() {
            p.hash(&mut hasher)
        }
        hasher.finish()
    }

    fn var_requests_hash(var_requests: &[api::VarRequest]) -> u64 {
        let mut hasher = DefaultHasher::new();
        var_requests.hash(&mut hasher);
        hasher.finish()
    }

    fn with_options(&self, options: &[(String, String)]) -> Self {
        Self::new(
            self.pkg_requests_hash,
            self.var_requests_hash,
            self.packages_hash,
            StateId::options_hash(options),
        )
    }

    fn with_pkg_requests(&self, pkg_requests: &[api::PkgRequest]) -> Self {
        Self::new(
            StateId::pkg_requests_hash(pkg_requests),
            self.var_requests_hash,
            self.packages_hash,
            self.options_hash,
        )
    }

    fn with_packages(&self, packages: &[(Arc<api::Spec>, PackageSource)]) -> Self {
        Self::new(
            self.pkg_requests_hash,
            self.var_requests_hash,
            StateId::packages_hash(packages),
            self.options_hash,
        )
    }

    fn with_var_requests_and_options(
        &self,
        var_requests: &[api::VarRequest],
        options: &[(String, String)],
    ) -> Self {
        Self::new(
            self.pkg_requests_hash,
            StateId::var_requests_hash(var_requests),
            self.packages_hash,
            StateId::options_hash(options),
        )
    }
}

// `State` is immutable. It should not derive Clone.
#[derive(Debug)]
pub struct State {
    pub pkg_requests: Arc<Vec<api::PkgRequest>>,
    var_requests: Arc<Vec<api::VarRequest>>,
    packages: Arc<Vec<(Arc<api::Spec>, PackageSource)>>,
    options: Arc<Vec<(String, String)>>,
    state_id: StateId,
}

impl State {
    pub fn new(
        pkg_requests: Vec<api::PkgRequest>,
        var_requests: Vec<api::VarRequest>,
        packages: Vec<(Arc<api::Spec>, PackageSource)>,
        options: Vec<(String, String)>,
    ) -> Self {
        // TODO: This pre-calculates the hash but there
        // may be states constructed where the id is
        // never accessed. Determine if it is better
        // to lazily compute this on demand.
        let state_id = StateId::new(
            StateId::pkg_requests_hash(&pkg_requests),
            StateId::var_requests_hash(&var_requests),
            StateId::packages_hash(&packages),
            StateId::options_hash(&options),
        );
        State {
            pkg_requests: Arc::new(pkg_requests),
            var_requests: Arc::new(var_requests),
            packages: Arc::new(packages),
            options: Arc::new(options),
            state_id,
        }
    }

    pub fn as_solution(&self) -> Result<Solution> {
        let mut solution = Solution::new(Some(self.options.iter().cloned().collect()));
        for (spec, source) in self.packages.iter() {
            let req = self
                .get_merged_request(spec.pkg.name())
                .map_err(GraphError::RequestError)?;
            solution.add(&req, spec.clone(), source.clone());
        }
        Ok(solution)
    }

    pub fn default() -> Self {
        State::new(
            Vec::default(),
            Vec::default(),
            Vec::default(),
            Vec::default(),
        )
    }

    pub fn get_current_resolve(
        &self,
        name: &str,
    ) -> errors::GetCurrentResolveResult<(&Arc<api::Spec>, &PackageSource)> {
        // TODO: cache this
        for (spec, source) in &*self.packages {
            if spec.pkg.name() == name {
                return Ok((spec, source));
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

    pub fn get_next_request(&self) -> Result<Option<api::PkgRequest>> {
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

    pub fn get_pkg_requests(&self) -> &Vec<api::PkgRequest> {
        &self.pkg_requests
    }

    pub fn get_var_requests(&self) -> &Vec<api::VarRequest> {
        &self.var_requests
    }

    fn with_options(&self, options: Vec<(String, String)>) -> Self {
        let state_id = self.state_id.with_options(&options);
        Self {
            pkg_requests: Arc::clone(&self.pkg_requests),
            var_requests: Arc::clone(&self.var_requests),
            packages: Arc::clone(&self.packages),
            options: Arc::new(options),
            state_id,
        }
    }

    fn with_package(&self, spec: Arc<api::Spec>, source: PackageSource) -> Self {
        let mut packages = Vec::with_capacity(self.packages.len() + 1);
        packages.extend(self.packages.iter().cloned());
        packages.push((spec, source));
        let state_id = self.state_id.with_packages(&packages);
        Self {
            pkg_requests: Arc::clone(&self.pkg_requests),
            var_requests: Arc::clone(&self.var_requests),
            packages: Arc::new(packages),
            options: Arc::clone(&self.options),
            state_id,
        }
    }

    fn with_pkg_requests(&self, pkg_requests: Vec<api::PkgRequest>) -> Self {
        let state_id = self.state_id.with_pkg_requests(&pkg_requests);
        Self {
            pkg_requests: Arc::new(pkg_requests),
            var_requests: Arc::clone(&self.var_requests),
            packages: Arc::clone(&self.packages),
            options: Arc::clone(&self.options),
            state_id,
        }
    }

    fn with_var_requests_and_options(
        &self,
        var_requests: Vec<api::VarRequest>,
        options: Vec<(String, String)>,
    ) -> Self {
        let state_id = self
            .state_id
            .with_var_requests_and_options(&var_requests, &options);
        Self {
            pkg_requests: Arc::clone(&self.pkg_requests),
            var_requests: Arc::new(var_requests),
            packages: Arc::clone(&self.packages),
            options: Arc::new(options),
            state_id,
        }
    }

    pub fn get_option_map(&self) -> api::OptionMap {
        // TODO: cache this
        self.options.iter().cloned().collect()
    }

    pub fn id(&self) -> u64 {
        self.state_id.id()
    }
}

#[derive(Clone, Debug)]
pub enum SkipPackageNoteReason {
    String(String),
    Compatibility(api::Compatibility),
}

impl std::fmt::Display for SkipPackageNoteReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => s.fmt(f),
            Self::Compatibility(c) => c.fmt(f),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SkipPackageNote {
    pub pkg: Ident,
    pub reason: SkipPackageNoteReason,
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

#[derive(Clone, Debug)]
pub struct StepBack {
    pub cause: String,
    pub destination: Arc<State>,
}

impl StepBack {
    pub fn new(cause: impl Into<String>, to: &Arc<State>) -> Self {
        StepBack {
            cause: cause.into(),
            destination: Arc::clone(to),
        }
    }

    pub fn apply(&self, _base: &State) -> Arc<State> {
        Arc::clone(&self.destination)
    }
}
