// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use async_stream::stream;
use futures::Stream;
use once_cell::sync::{Lazy, OnceCell};
use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::iter::FromIterator;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;

use crate::api::{self, Ident, InclusionPolicy, OptNameBuf, PkgName, PkgNameBuf};

use super::errors::{self};
use super::{
    package_iterator::PackageIterator,
    solution::{PackageSource, Solution},
};

#[cfg(test)]
#[path = "./graph_test.rs"]
mod graph_test;

pub static DEAD_STATE: Lazy<Arc<State>> = Lazy::new(State::default);

const BRANCH_ALREADY_ATTEMPTED: &str = "Branch already attempted";

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum GraphError {
    #[error("Recursion error: {0}")]
    RecursionError(&'static str),
    #[error(transparent)]
    RequestError(#[from] errors::GetMergedRequestError),
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
    pub fn apply(&self, parent: &Arc<State>, base: &Arc<State>) -> Arc<State> {
        match self {
            Change::RequestPackage(rp) => rp.apply(parent, base),
            Change::RequestVar(rv) => rv.apply(parent, base),
            Change::SetOptions(so) => so.apply(parent, base),
            Change::SetPackage(sp) => sp.apply(parent, base),
            Change::SetPackageBuild(spb) => spb.apply(parent, base),
            Change::StepBack(sb) => sb.apply(parent, base),
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
        for change in self.changes.iter() {
            state = Some(change.apply(base, state.as_ref().unwrap_or(base)));
        }
        state.unwrap_or_else(|| Arc::clone(base))
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
            req.to_mut()
                .pkg
                .components
                .insert(api::Component::default_for_run());
        }

        // Add the package that would make this request, into the request
        req.to_mut()
            .add_requester(api::RequestedBy::PackageBuild(self.spec.pkg.clone()));

        let mut changes = vec![Change::RequestPackage(RequestPackage::new(
            req.clone().into_owned(),
        ))];
        // we need to check if this request will change a previously
        // resolved package (eg: by adding a new component with new requirements)
        let (spec, _source) = match self.base.get_current_resolve(&req.pkg.name) {
            Ok(e) => e,
            Err(_) => return changes,
        };
        let existing = match self.base.get_merged_request(&req.pkg.name) {
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
                        embedded.pkg.clone(),
                        api::RequestedBy::PackageBuild(self.spec.pkg.clone()),
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
            spec.pkg.name.as_opt_name().to_owned(),
            spec.compat.render(&spec.pkg.version),
        );
        for opt in &spec.build.options {
            let value = opt.get_value(None);
            if !value.is_empty() {
                let name = opt.full_name().with_default_namespace(&spec.pkg.name);
                opts.insert(name, value);
            }
        }
        Change::SetOptions(SetOptions::new(opts))
    }
}

#[derive(Clone, Debug, Error)]
#[error("Failed to resolve")]
pub struct Graph {
    pub root: Arc<tokio::sync::RwLock<Arc<Node>>>,
    pub nodes: HashMap<u64, Arc<tokio::sync::RwLock<Arc<Node>>>>,
}

impl Graph {
    pub fn new() -> Self {
        let dead_state = Arc::clone(&*DEAD_STATE);
        let dead_state_id = dead_state.id();
        let dead_state = Arc::new(tokio::sync::RwLock::new(Arc::new(Node::new(dead_state))));
        let nodes = [(dead_state_id, dead_state.clone())]
            .iter()
            .cloned()
            .collect();
        Graph {
            root: dead_state,
            nodes,
        }
    }

    pub async fn add_branch(
        &mut self,
        source_id: u64,
        decision: Arc<Decision>,
    ) -> Result<Arc<tokio::sync::RwLock<Arc<Node>>>> {
        let old_node = self
            .nodes
            .get(&source_id)
            .expect("source_id exists in nodes")
            .clone();
        let new_state = decision.apply(&(old_node.read().await.state));
        let mut new_node = Arc::new(tokio::sync::RwLock::new(Arc::new(Node::new(new_state))));
        {
            let mut new_node_lock = new_node.write().await;

            match self.nodes.get(&new_node_lock.id()) {
                None => {
                    self.nodes.insert(new_node_lock.id(), new_node.clone());
                    for (name, iterator) in old_node.read().await.iterators.iter() {
                        Arc::make_mut(&mut new_node_lock)
                            .set_iterator(name.clone(), iterator)
                            .await
                    }
                }
                Some(node) => {
                    drop(new_node_lock);
                    new_node = node.clone();
                }
            }
        }

        // Don't record `StepBack` changes into the graph. Doing so will
        // preclude revisiting a `Node` that has unvisited child states.
        if !(decision.changes.len() == 1
            && matches!(
                unsafe { decision.changes.get(0).unwrap_unchecked() },
                Change::StepBack(_)
            ))
        {
            let mut old_node_lock = old_node.write().await;
            {
                // Avoid deadlock if old_node is the same node as new_node
                if !Arc::ptr_eq(&old_node, &new_node) {
                    let mut new_node_lock = new_node.write().await;
                    Arc::make_mut(&mut old_node_lock)
                        .add_output(decision.clone(), &new_node_lock.state)?;
                    Arc::make_mut(&mut new_node_lock).add_input(&old_node_lock.state, decision);
                } else {
                    let old_state = old_node_lock.state.clone();
                    Arc::make_mut(&mut old_node_lock).add_output(decision.clone(), &old_state)?;
                    Arc::make_mut(&mut old_node_lock).add_input(&old_state, decision);
                }
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
    YieldNextNode(Arc<Decision>),
}

pub struct GraphIter<'graph> {
    graph: &'graph Graph,
    node_outputs: HashMap<u64, VecDeque<Arc<Decision>>>,
    to_process: VecDeque<Arc<tokio::sync::RwLock<Arc<Node>>>>,
    /// Which entry of node_outputs is currently being worked on.
    outs: Option<u64>,
    iter_node: Arc<tokio::sync::RwLock<Arc<Node>>>,
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

    pub fn iter<'a: 'graph>(&'a mut self) -> impl Stream<Item = (Arc<Node>, Arc<Decision>)> + 'a {
        stream! {
            'outer: loop {
                match self.walk_state {
                    WalkState::ToProcessNonEmpty => {
                        if self.to_process.is_empty() {
                            self.walk_state = WalkState::YieldOuts;
                            continue 'outer;
                        }

                        let node = self.to_process.pop_front().unwrap();
                        let node_lock = node.read().await;

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
                            continue 'outer;
                        }

                        self.to_process.push_back(node.clone());
                        let decision = outs.pop_front().unwrap();
                        yield (node_lock.clone(), decision);
                        continue 'outer;
                    }
                    WalkState::YieldOuts => {
                        let outs = self.node_outputs.get_mut(&self.outs.unwrap()).unwrap();
                        if outs.is_empty() {
                            break 'outer;
                        }

                        let node_lock = self.iter_node.read().await;

                        let decision = outs.pop_front().unwrap();
                        self.walk_state = WalkState::YieldNextNode(decision.clone());
                        yield (node_lock.clone(), decision);
                        continue 'outer;
                    }
                    WalkState::YieldNextNode(ref decision) => {
                        let next_state_id = {
                            let node_lock = self.iter_node.read().await;

                            let next_state = decision.apply(&node_lock.state);
                            next_state.id()
                        };
                        self.iter_node = self.graph.nodes.get(&next_state_id).unwrap().clone();
                        self.walk_state = WalkState::YieldOuts;
                        continue 'outer;
                    }
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
    inputs_decisions: Vec<Arc<Decision>>,
    outputs: HashSet<u64>,
    outputs_decisions: Vec<Arc<Decision>>,
    pub state: Arc<State>,
    iterators: HashMap<PkgNameBuf, Arc<tokio::sync::Mutex<Box<dyn PackageIterator + Send>>>>,
}

impl Node {
    pub fn add_input(&mut self, state: &State, decision: Arc<Decision>) {
        self.inputs.insert(state.id());
        self.inputs_decisions.push(decision);
    }

    pub fn add_output(&mut self, decision: Arc<Decision>, state: &State) -> Result<()> {
        if self.outputs.contains(&state.id()) {
            return Err(GraphError::RecursionError(BRANCH_ALREADY_ATTEMPTED));
        }
        self.outputs.insert(state.id());
        self.outputs_decisions.push(decision);
        Ok(())
    }

    pub fn get_iterator(
        &self,
        package_name: &PkgName,
    ) -> Option<Arc<tokio::sync::Mutex<Box<dyn PackageIterator + Send>>>> {
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

    pub async fn set_iterator(
        &mut self,
        package_name: api::PkgNameBuf,
        iterator: &Arc<tokio::sync::Mutex<Box<dyn PackageIterator + Send>>>,
    ) {
        if self.iterators.contains_key(&package_name) {
            tracing::error!("iterator already exists [INTERNAL ERROR]");
            debug_assert!(false, "iterator already exists [INTERNAL ERROR]");
        }
        self.iterators.insert(
            package_name,
            Arc::new(tokio::sync::Mutex::new(
                iterator.lock().await.async_clone().await,
            )),
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

/// For counting requests for the same package, but not necessarily
/// the same version, e.g. gcc/9 and gcc/6 are the same package.
pub static REQUESTS_FOR_SAME_PACKAGE_COUNT: AtomicU64 = AtomicU64::new(0);
/// For counting completely identical duplicate requests that appear,
/// e.g. gcc/~9.3 and gcc/~9.3, but not gcc/6
pub static DUPLICATE_REQUESTS_COUNT: AtomicU64 = AtomicU64::new(0);

impl RequestPackage {
    pub fn new(request: api::PkgRequest) -> Self {
        RequestPackage { request }
    }

    pub fn apply(&self, parent: &Arc<State>, base: &Arc<State>) -> Arc<State> {
        // XXX: An immutable data structure for pkg_requests would
        // allow for sharing.
        let mut cloned_request = Some(self.request.clone());
        let mut new_requests = base
            .pkg_requests
            .iter()
            .cloned()
            .map(|existing_request| {
                // `restrict` doesn't check the package name!
                if cloned_request.is_none() || existing_request.pkg.name != self.request.pkg.name {
                    existing_request
                } else {
                    // Safety: `cloned_request` is not `None` by previous test.
                    let mut request = unsafe { cloned_request.take().unwrap_unchecked() };
                    match request.restrict(&existing_request) {
                        Ok(_) => Arc::new(request.into()),
                        Err(_) => {
                            // Keep looking
                            cloned_request = Some(request);
                            existing_request
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        if cloned_request.is_some() {
            // No candidate was found to merge with; append.

            // If the new request is for a package that there is already a
            // request for, then we want the solver to look at resolving
            // the requests for this package sooner rather than later.
            // This is done by moving those requests to the front of the
            // request list.
            //
            // It helps counter resolving delays for common packages
            // caused by: 1) resolving a package with lots of dependencies
            // many of which resolve to packages that have varying
            // requests for the same lower-level package, and 2)
            // IfAlreadyPresent requests. These situations can exacerbate
            // the creation of merged requests with impossible to satisfy rules
            // that result in large amounts of backtracking across many
            // levels (20+) of the search.

            // Set up flags for checking things during the sort, they will
            // be used after the sort to add to stats counters.
            let mut existing_request_for_package = false;
            let mut duplicate_request = false;

            // Move any requests for the same package to the front of the
            // list by sorting based on whether a request is for the same
            // package as the new request or not.
            //
            // This sort is stable, so existing requests for the package
            // will remain in the same relative order as they were, as
            // will requests for other packages.
            new_requests.sort_by_cached_key(|req| -> i32 {
                if req.pkg.name() == self.request.pkg.name() {
                    // There is already a request in the list for the same
                    // package as the new request's package.
                    existing_request_for_package = true;

                    // Check if the new request is a completely identical
                    // duplicate request.
                    if req.pkg == self.request.pkg {
                        duplicate_request = true;
                    }

                    // It's the same package, move it towards the front
                    0
                } else {
                    // It's a different package, leave it where it is
                    1
                }
            });

            // Update the counters with what was found during the sort.
            if existing_request_for_package {
                REQUESTS_FOR_SAME_PACKAGE_COUNT.fetch_add(1, Ordering::SeqCst);
            }
            if duplicate_request {
                DUPLICATE_REQUESTS_COUNT.fetch_add(1, Ordering::SeqCst);
            }

            // Add the new request to the end. This is ok because the
            // other requests for this package are at the front of the
            // list now. If this package needs resolving, this new request
            // will be added to the merged request when this package is
            // next selected by the solver.
            new_requests.push(Arc::new(self.request.clone().into()));
        }

        Arc::new(base.with_pkg_requests(parent, new_requests))
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

    pub fn apply(&self, parent: &Arc<State>, base: &Arc<State>) -> Arc<State> {
        // XXX: An immutable data structure for var_requests would
        // allow for sharing.
        let mut new_requests = Arc::clone(&base.var_requests);
        // Avoid adding duplicate var requests.
        if !base.contains_var_request(&self.request) {
            Arc::make_mut(&mut new_requests).insert(self.request.clone());
        }
        let options = SetOptions::compute_new_options(
            &*base,
            vec![(&self.request.var, &self.request.value)].into_iter(),
            true,
        );
        Arc::new(base.with_var_requests_and_options(parent, new_requests, options))
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

    pub fn apply(&self, parent: &Arc<State>, base: &Arc<State>) -> Arc<State> {
        let new_options = Self::compute_new_options(&*base, self.options.iter(), false);
        Arc::new(base.with_options(parent, new_options))
    }

    /// Compute the new options list for a state, preserving the insertion
    /// order based on option key.
    pub fn compute_new_options<'i, I>(
        base: &State,
        new_options: I,
        update_existing_option_with_empty_value: bool,
    ) -> BTreeMap<OptNameBuf, String>
    where
        I: Iterator<Item = (&'i OptNameBuf, &'i String)>,
    {
        let mut options = (*base.options).clone();
        // Update base options with request options...
        for (k, v) in new_options {
            match options.get_mut(k) {
                // Unless already present and request option value is empty.
                Some(_) if v.is_empty() && !update_existing_option_with_empty_value => continue,
                // If option already existed, change the value
                Some(value) => *value = v.to_owned(),
                None => {
                    options.insert(k.to_owned(), v.to_owned());
                }
            };
        }
        options
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

    pub fn apply(&self, parent: &Arc<State>, base: &Arc<State>) -> Arc<State> {
        Arc::new(base.append_package(Some(parent), self.spec.clone(), self.source.clone()))
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

    pub fn apply(&self, parent: &Arc<State>, base: &Arc<State>) -> Arc<State> {
        Arc::new(base.append_package(Some(parent), self.spec.clone(), self.source.clone()))
    }
}

#[derive(Clone, Debug)]
struct StateId {
    pkg_requests_hash: u64,
    var_requests_hash: u64,
    // A set of what `VarRequest` hashes exist in this `StateId`.
    var_requests_membership: Arc<HashSet<u64>>,
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
        var_requests_membership: Arc<HashSet<u64>>,
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
            var_requests_membership,
            packages_hash,
            options_hash,
            full_hash,
        }
    }

    fn options_hash(options: &BTreeMap<OptNameBuf, String>) -> u64 {
        let mut hasher = DefaultHasher::new();
        options.hash(&mut hasher);
        hasher.finish()
    }

    fn pkg_requests_hash(pkg_requests: &Vec<Arc<CachedHash<api::PkgRequest>>>) -> u64 {
        let mut hasher = DefaultHasher::new();
        pkg_requests.hash(&mut hasher);
        hasher.finish()
    }

    fn packages_hash(packages: &StatePackages) -> u64 {
        let mut hasher = DefaultHasher::new();
        for (spec, _) in packages.values() {
            spec.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn var_requests_hash(var_requests: &BTreeSet<api::VarRequest>) -> (u64, HashSet<u64>) {
        let mut var_requests_membership = HashSet::new();
        let mut global_hasher = DefaultHasher::new();
        for var_request in var_requests {
            // First hash this individual `VarRequest`
            let mut hasher = DefaultHasher::new();
            var_request.hash(&mut hasher);
            let single_var_request_hash = hasher.finish();
            var_requests_membership.insert(single_var_request_hash);

            // The "global" hash is a hash of hashes
            single_var_request_hash.hash(&mut global_hasher);
        }
        (global_hasher.finish(), var_requests_membership)
    }

    fn with_options(&self, options: &BTreeMap<OptNameBuf, String>) -> Self {
        Self::new(
            self.pkg_requests_hash,
            self.var_requests_hash,
            Arc::clone(&self.var_requests_membership),
            self.packages_hash,
            StateId::options_hash(options),
        )
    }

    fn with_pkg_requests(&self, pkg_requests: &Vec<Arc<CachedHash<api::PkgRequest>>>) -> Self {
        Self::new(
            StateId::pkg_requests_hash(pkg_requests),
            self.var_requests_hash,
            Arc::clone(&self.var_requests_membership),
            self.packages_hash,
            self.options_hash,
        )
    }

    fn with_packages(&self, packages: &StatePackages) -> Self {
        Self::new(
            self.pkg_requests_hash,
            self.var_requests_hash,
            Arc::clone(&self.var_requests_membership),
            StateId::packages_hash(packages),
            self.options_hash,
        )
    }

    fn with_var_requests_and_options(
        &self,
        var_requests: &BTreeSet<api::VarRequest>,
        options: &BTreeMap<OptNameBuf, String>,
    ) -> Self {
        let (var_requests_hash, var_requests_membership) = StateId::var_requests_hash(var_requests);
        Self::new(
            self.pkg_requests_hash,
            var_requests_hash,
            Arc::new(var_requests_membership),
            self.packages_hash,
            StateId::options_hash(options),
        )
    }
}

/// For caching the hash of an `api::PkgRequest`.
///
/// Computing the hash of `api::PkgRequest` represents a significant portion
/// of solver runtime.
#[derive(Clone, Debug)]
pub struct CachedHash<T> {
    object: T,
    hash: u64,
}

impl<T> std::ops::Deref for CachedHash<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.object
    }
}

impl<T: Hash> From<T> for CachedHash<T> {
    fn from(object: T) -> Self {
        let mut hasher = DefaultHasher::new();
        object.hash(&mut hasher);
        let hash = hasher.finish();

        Self { object, hash }
    }
}

impl<T> std::hash::Hash for CachedHash<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

type StatePackages = Arc<BTreeMap<PkgNameBuf, (CachedHash<Arc<api::Spec>>, PackageSource)>>;

// `State` is immutable. It should not derive Clone.
#[derive(Debug)]
pub struct State {
    pkg_requests: Arc<Vec<Arc<CachedHash<api::PkgRequest>>>>,
    var_requests: Arc<BTreeSet<api::VarRequest>>,
    packages: StatePackages,
    options: Arc<BTreeMap<OptNameBuf, String>>,
    state_id: StateId,
    cached_option_map: Arc<OnceCell<api::OptionMap>>,
    // How deep is this state?
    pub(crate) state_depth: u64,
}

impl State {
    pub fn new(
        pkg_requests: Vec<api::PkgRequest>,
        var_requests: Vec<api::VarRequest>,
        packages: Vec<(Arc<api::Spec>, PackageSource)>,
        options: Vec<(api::OptNameBuf, String)>,
    ) -> Arc<Self> {
        // TODO: This pre-calculates the hash but there
        // may be states constructed where the id is
        // never accessed. Determine if it is better
        // to lazily compute this on demand.
        let pkg_requests = pkg_requests
            .into_iter()
            .map(|el| Arc::new(el.into()))
            .collect();
        let var_requests = var_requests.into_iter().collect();
        let options = options.into_iter().collect();
        let (var_requests_hash, var_requests_membership) =
            StateId::var_requests_hash(&var_requests);
        let state_id = StateId::new(
            StateId::pkg_requests_hash(&pkg_requests),
            var_requests_hash,
            Arc::new(var_requests_membership),
            0,
            StateId::options_hash(&options),
        );
        let mut s = State {
            pkg_requests: Arc::new(pkg_requests),
            var_requests: Arc::new(var_requests),
            packages: Arc::new(BTreeMap::new()),
            options: Arc::new(options),
            state_id,
            cached_option_map: Arc::new(OnceCell::new()),
            state_depth: 0,
        };
        for (package, source) in packages.into_iter() {
            s = s.append_package(None, package, source)
        }
        Arc::new(s)
    }

    pub fn as_solution(&self) -> Result<Solution> {
        let mut solution = Solution::new(Some((&self.options).into()));
        for (spec, source) in self.packages.values() {
            let req = self
                .get_merged_request(&spec.pkg.name)
                .map_err(GraphError::RequestError)?;
            solution.add(&req, Arc::clone(&*spec), source.clone());
        }
        Ok(solution)
    }

    /// Return true if this state already contains this request.
    pub fn contains_var_request(&self, var_request: &api::VarRequest) -> bool {
        let mut hasher = DefaultHasher::new();
        var_request.hash(&mut hasher);
        let single_var_request_hash = hasher.finish();
        self.state_id
            .var_requests_membership
            .contains(&single_var_request_hash)
    }

    pub fn default() -> Arc<Self> {
        State::new(
            Vec::default(),
            Vec::default(),
            Vec::default(),
            Vec::default(),
        )
    }

    pub fn get_current_resolve(
        &self,
        name: &PkgName,
    ) -> errors::GetCurrentResolveResult<(&CachedHash<Arc<api::Spec>>, &PackageSource)> {
        self.packages.get(name).map(|(s, p)| (s, p)).ok_or_else(|| {
            errors::GetCurrentResolveError::PackageNotResolved(format!(
                "Has not been resolved: '{}'",
                name
            ))
        })
    }

    pub fn get_merged_request(
        &self,
        name: &PkgName,
    ) -> errors::GetMergedRequestResult<api::PkgRequest> {
        // tests reveal this method is not safe to cache.
        let mut merged: Option<api::PkgRequest> = None;
        for request in self.pkg_requests.iter() {
            match merged.as_mut() {
                None => {
                    if &*request.pkg.name != name {
                        continue;
                    }
                    merged = Some((***request).clone());
                }
                Some(merged) => {
                    if request.pkg.name != merged.pkg.name {
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
        // Note: The next request this returns may not be as expected
        // due to the interaction of multiple requests and
        // 'IfAlreadyPresent' requests.
        //
        // TODO: consider changing the request list to only contain
        // requests that have not been satisfied, or only merged
        // requests, or both.
        for request in self.pkg_requests.iter() {
            if self.packages.contains_key(&*request.pkg.name) {
                continue;
            }
            if request.inclusion_policy == InclusionPolicy::IfAlreadyPresent {
                // This request doesn't need to be looked at yet. It
                // will be picked up eventually, by the
                // get_merged_request() call below, if there is a
                // non-'IfAlreadyPresent' request for the same package
                // in pkg_requests. This tends to delay resolving
                // these requests until later in the solve. It stops
                // the solver following a strict breadth first
                // expansion of dependencies.
                continue;
            }
            return Ok(Some(self.get_merged_request(&request.pkg.name)?));
        }

        Ok(None)
    }

    pub fn get_pkg_requests(&self) -> &Vec<Arc<CachedHash<api::PkgRequest>>> {
        &self.pkg_requests
    }

    pub fn get_var_requests(&self) -> &BTreeSet<api::VarRequest> {
        &self.var_requests
    }

    pub fn get_resolved_packages(
        &self,
    ) -> &BTreeMap<PkgNameBuf, (CachedHash<Arc<api::Spec>>, PackageSource)> {
        &self.packages
    }

    fn with_options(&self, parent: &Self, options: BTreeMap<OptNameBuf, String>) -> Self {
        let state_id = self.state_id.with_options(&options);
        Self {
            pkg_requests: Arc::clone(&self.pkg_requests),
            var_requests: Arc::clone(&self.var_requests),
            packages: Arc::clone(&self.packages),
            options: Arc::new(options),
            state_id,
            // options are changing
            cached_option_map: Arc::new(OnceCell::new()),
            state_depth: parent.state_depth + 1,
        }
    }

    fn append_package(
        &self,
        parent: Option<&Arc<Self>>,
        spec: Arc<api::Spec>,
        source: PackageSource,
    ) -> Self {
        let mut packages = Arc::clone(&self.packages);
        Arc::make_mut(&mut packages).insert(spec.pkg.name.clone(), (spec.into(), source));
        let state_id = self.state_id.with_packages(&packages);
        Self {
            pkg_requests: Arc::clone(&self.pkg_requests),
            var_requests: Arc::clone(&self.var_requests),
            packages,
            options: Arc::clone(&self.options),
            state_id,
            // options are the same
            cached_option_map: Arc::clone(&self.cached_option_map),
            state_depth: parent.as_ref().map(|p| p.state_depth + 1).unwrap_or(0),
        }
    }

    fn with_pkg_requests(
        &self,
        parent: &Self,
        pkg_requests: Vec<Arc<CachedHash<api::PkgRequest>>>,
    ) -> Self {
        let state_id = self.state_id.with_pkg_requests(&pkg_requests);
        Self {
            pkg_requests: Arc::new(pkg_requests),
            var_requests: Arc::clone(&self.var_requests),
            packages: Arc::clone(&self.packages),
            options: Arc::clone(&self.options),
            state_id,
            // options are the same
            cached_option_map: Arc::clone(&self.cached_option_map),
            state_depth: parent.state_depth + 1,
        }
    }

    fn with_var_requests_and_options(
        &self,
        parent: &Self,
        var_requests: Arc<BTreeSet<api::VarRequest>>,
        options: BTreeMap<OptNameBuf, String>,
    ) -> Self {
        let state_id = self
            .state_id
            .with_var_requests_and_options(&var_requests, &options);
        Self {
            pkg_requests: Arc::clone(&self.pkg_requests),
            var_requests,
            packages: Arc::clone(&self.packages),
            options: Arc::new(options),
            state_id,
            // options are changing
            cached_option_map: Arc::new(OnceCell::new()),
            state_depth: parent.state_depth + 1,
        }
    }

    pub fn get_option_map(&self) -> &api::OptionMap {
        self.cached_option_map
            .get_or_init(|| (&self.options).into())
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
    // For counting the number of StepBack apply() calls
    global_counter: Arc<AtomicU64>,
}

impl StepBack {
    pub fn new(cause: impl Into<String>, to: &Arc<State>, global_counter: Arc<AtomicU64>) -> Self {
        StepBack {
            cause: cause.into(),
            destination: Arc::clone(to),
            global_counter,
        }
    }

    pub fn apply(&self, _parent: &Arc<State>, _base: &Arc<State>) -> Arc<State> {
        // Increment the counter before restoring the state
        self.global_counter.fetch_add(1, Ordering::SeqCst);
        Arc::clone(&self.destination)
    }
}
