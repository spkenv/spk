// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::iter::FromIterator;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_stream::stream;
use colored::Colorize;
use futures::Stream;
use miette::Diagnostic;
use once_cell::sync::{Lazy, OnceCell};
use spk_schema::foundation::format::{FormatChange, FormatIdent, FormatOptionMap, FormatRequest};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{OptNameBuf, PkgName, PkgNameBuf};
use spk_schema::foundation::option_map;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::version::Compatibility;
use spk_schema::ident::{InclusionPolicy, PkgRequest, Request, RequestedBy, VarRequest};
use spk_schema::prelude::*;
use spk_schema::{
    AnyIdent,
    BuildIdent,
    ComponentSpecList,
    EmbeddedPackagesList,
    RequirementsList,
    Spec,
    SpecRecipe,
};
use spk_solve_package_iterator::{PackageIterator, PromotionPatterns};
use spk_solve_solution::{PackageSource, Solution};
use thiserror::Error;

#[cfg(test)]
#[path = "./graph_test.rs"]
mod graph_test;

pub static DEAD_STATE: Lazy<Arc<State>> = Lazy::new(State::default_state);

const BRANCH_ALREADY_ATTEMPTED: &str = "Branch already attempted";

/// Allow the request order found as defined in package specs to be reordered,
/// moving package names that match entries in this list of patterns to the
/// front of the request list.
static REQUESTS_PRIORITY_ORDER: Lazy<PromotionPatterns> = Lazy::new(|| {
    PromotionPatterns::new(
        spk_config::get_config()
            .map(|c| c.solver.request_priority_order.clone())
            .unwrap_or_else(|_| "".to_string())
            .as_ref(),
    )
});

#[derive(Diagnostic, Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum GraphError {
    #[error("Recursion error: {0}")]
    RecursionError(&'static str),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    RequestError(#[from] super::error::GetMergedRequestError),
}

pub type Result<T> = std::result::Result<T, GraphError>;

#[derive(Clone, Debug)]
pub enum Change {
    RequestPackage(RequestPackage),
    RequestVar(RequestVar),
    SetOptions(SetOptions),
    /// Adds a package to the solution. The package must have already been
    /// checked that it is compatible with the current solution and valid to
    /// be added.
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

    pub fn as_decision(&self) -> Decision {
        Decision {
            changes: vec![self.clone()],
            notes: Vec::default(),
        }
    }

    fn get_request_change_label(level: u64) -> &'static str {
        if level == PkgRequest::INITIAL_REQUESTS_LEVEL {
            "INITIAL REQUEST"
        } else {
            "REQUEST"
        }
    }
}

impl FormatChange for Change {
    type State = State;

    fn format_change(
        &self,
        format_settings: &spk_schema::foundation::format::FormatChangeOptions,
        state: Option<&Self::State>,
    ) -> String {
        use Change::*;
        match self {
            RequestPackage(c) => {
                format!(
                    "{} {}",
                    Self::get_request_change_label(format_settings.level).blue(),
                    c.request.format_request(
                        c.request.pkg.repository_name.as_ref(),
                        &c.request.pkg.name,
                        format_settings
                    )
                )
            }
            RequestVar(c) => {
                format!(
                    "{} {}{}",
                    Self::get_request_change_label(format_settings.level).blue(),
                    option_map! {c.request.var.clone() => c.request.value.as_pinned().unwrap_or_default()}
                        .format_option_map(),
                    if format_settings.verbosity > PkgRequest::SHOW_REQUEST_DETAILS {
                        format!(
                            " fromBuildEnv: {}",
                            c.request.value.is_from_build_env().to_string().cyan()
                        )
                    } else {
                        "".to_string()
                    }
                )
            }
            SetPackageBuild(c) => {
                format!("{} {}", "BUILD".yellow(), c.spec.ident().format_ident())
            }
            SetPackage(c) => {
                if format_settings.verbosity > 0 {
                    // Work out who the requesters were, so this can show
                    // the resolved package and its requester(s)
                    let requested_by: Vec<String> = match state {
                        Some(s) => match s.get_merged_request(c.spec.name()) {
                            Ok(r) => r.get_requesters().iter().map(ToString::to_string).collect(),
                            Err(_) => {
                                match &c.source {
                                    // This happens with embedded requests because they are
                                    // requested and added in the same state during a solve.
                                    // We can use their PackageSource data to find what
                                    // requested them.
                                    PackageSource::BuildFromSource { recipe } => {
                                        vec![RequestedBy::PackageVersion(recipe.ident().clone())
                                            .to_string()]
                                    }
                                    PackageSource::Embedded { parent } => {
                                        vec![RequestedBy::Embedded(parent.clone()).to_string()]
                                    }
                                    _ => {
                                        // Don't think this should happen
                                        vec![RequestedBy::Unknown.to_string()]
                                    }
                                }
                            }
                        },
                        None => {
                            vec![RequestedBy::NoState.to_string()]
                        }
                    };

                    // Show the resolved package and its requester(s)
                    format!(
                        "{} {}  (requested by {})",
                        "RESOLVE".green(),
                        c.spec.ident().format_ident(),
                        requested_by.join(", ")
                    )
                } else {
                    // Just show the resolved package, don't show the requester(s)
                    format!("{} {}", "RESOLVE".green(), c.spec.ident().format_ident())
                }
            }
            SetOptions(c) => {
                format!("{} {}", "ASSIGN".cyan(), c.options.format_option_map())
            }
            StepBack(c) => {
                format!("{} {}", "BLOCKED".red(), c.cause)
            }
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
    pub fn builder(base: &Arc<State>) -> DecisionBuilder<'_, 'static> {
        DecisionBuilder::new(base)
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
    base: &'state Arc<State>,
    components: HashSet<&'cmpt Component>,
}

impl<'state> DecisionBuilder<'state, 'static> {
    pub fn new(base: &'state Arc<State>) -> Self {
        Self {
            base,
            components: HashSet::new(),
        }
    }
}

impl<'state, 'cmpt> DecisionBuilder<'state, 'cmpt> {
    pub fn with_components<'a>(
        self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> DecisionBuilder<'state, 'a> {
        DecisionBuilder {
            components: components.into_iter().collect(),
            base: self.base,
        }
    }

    /// Create a new decision to build a package from a recipe.
    ///
    /// The returned decision describes building the given recipe
    /// into the package described by the given spec so that it
    /// can be included in the final solution.
    pub fn build_package(
        self,
        recipe: &Arc<SpecRecipe>,
        spec: &Arc<Spec>,
    ) -> crate::Result<Decision> {
        let generate_changes = || -> crate::Result<Vec<_>> {
            let mut changes = vec![Change::SetPackageBuild(Box::new(SetPackageBuild::new(
                Arc::clone(spec),
                Arc::clone(recipe),
            )))];

            let requester_ident: &BuildIdent = spec.ident();
            let requested_by = RequestedBy::PackageBuild(requester_ident.clone());
            changes
                .extend(self.requirements_to_changes(&spec.runtime_requirements(), &requested_by));
            changes.extend(self.components_to_changes(spec.components(), requester_ident));
            changes.extend(self.embedded_to_changes(spec.embedded(), requester_ident));
            changes.push(Self::options_to_change(spec));

            Ok(changes)
        };

        Ok(Decision {
            changes: generate_changes()?,
            notes: Vec::default(),
        })
    }

    /// Return all the changes needed when adding a package to the solution.
    fn set_package(&self, spec: Arc<Spec>, source: PackageSource) -> Vec<Change> {
        let mut changes = vec![Change::SetPackage(Box::new(SetPackage::new(
            Arc::clone(&spec),
            source,
        )))];

        let requester_ident: &BuildIdent = spec.ident();
        let requested_by = RequestedBy::PackageBuild(requester_ident.clone());
        changes.extend(self.requirements_to_changes(&spec.runtime_requirements(), &requested_by));
        changes.extend(self.components_to_changes(spec.components(), requester_ident));
        changes.extend(self.embedded_to_changes(spec.embedded(), requester_ident));
        changes.push(Self::options_to_change(&spec));

        changes
    }

    pub fn resolve_package(self, spec: &Arc<Spec>, source: PackageSource) -> Decision {
        Decision {
            changes: self.set_package(Arc::clone(spec), source),
            notes: Vec::default(),
        }
    }

    /// Make this package the next request to be considered.
    pub fn reconsider_package(
        self,
        request: PkgRequest,
        conflicting_package_name: &PkgName,
        counter: Arc<AtomicU64>,
    ) -> Decision {
        let generate_changes = || {
            let changes = vec![
                Change::StepBack(StepBack::new(
                    format!(
                        "Package {} embeds package already resolved: {conflicting_package_name}",
                        request.pkg.name
                    ),
                    self.base,
                    counter,
                )),
                Change::RequestPackage(RequestPackage::prioritize(request)),
            ];

            changes
        };

        Decision {
            changes: generate_changes(),
            notes: Vec::default(),
        }
    }

    fn requirements_to_changes(
        &self,
        requirements: &RequirementsList,
        requested_by: &RequestedBy,
    ) -> Vec<Change> {
        requirements
            .iter()
            .flat_map(|req| match req {
                Request::Pkg(req) => {
                    let mut req = req.clone();
                    req.add_requester(requested_by.clone());
                    self.pkg_request_to_changes(&req)
                }
                Request::Var(req) => vec![Change::RequestVar(RequestVar::new(req.clone()))],
            })
            .collect()
    }

    fn components_to_changes(
        &self,
        components: &ComponentSpecList,
        requester: &BuildIdent,
    ) -> Vec<Change> {
        let mut changes = vec![];
        let required = components.resolve_uses(self.components.iter().cloned());

        let requested_by = RequestedBy::PackageBuild(requester.clone());
        for component in components.iter() {
            if !required.contains(&component.name) {
                // TODO: is this check still necessary? We used to get required
                // using self.spec instead of components which might mean that this
                // is buggy now
                continue;
            }
            changes.extend(self.requirements_to_changes(&component.requirements, &requested_by));
            changes.extend(self.embedded_to_changes(&component.embedded, requester));
        }
        changes
    }

    fn pkg_request_to_changes(&self, req: &PkgRequest) -> Vec<Change> {
        let mut req = std::borrow::Cow::Borrowed(req);
        if req.pkg.components.is_empty() {
            // if no component was requested specifically,
            // then we must assume the default run component
            req.to_mut()
                .pkg
                .components
                .insert(Component::default_for_run());
        }

        let mut changes = vec![Change::RequestPackage(RequestPackage::new(
            req.clone().into_owned(),
        ))];
        // we need to check if this request will change a previously
        // resolved package (eg: by adding a new component with new requirements)
        let (spec, _source, _) = match self.base.get_current_resolve(&req.pkg.name) {
            Ok(e) => e,
            Err(_) => return changes,
        };
        let existing = match self.base.get_merged_request(&req.pkg.name) {
            Ok(r) => r,
            // the error case here should not be possible since we have
            // already found a resolved package...
            Err(_) => return changes,
        };
        let new_components = spec.components().resolve_uses(req.pkg.components.iter());
        let existing_components = spec
            .components()
            .resolve_uses(existing.pkg.components.iter());
        let added_components = new_components
            .difference(&existing_components)
            .collect::<HashSet<_>>();
        if added_components.is_empty() {
            return changes;
        }
        for component in spec.components().iter() {
            if !added_components.contains(&component.name) {
                continue;
            }
            let requested_by = RequestedBy::PackageBuild(spec.ident().clone());
            changes.extend(self.requirements_to_changes(&component.requirements, &requested_by));
        }
        changes
    }

    fn embedded_to_changes(
        &self,
        embedded: &EmbeddedPackagesList,
        parent: &BuildIdent,
    ) -> Vec<Change> {
        embedded
            .iter()
            .flat_map(|embedded| {
                let mut changes = vec![Change::RequestPackage(RequestPackage::new(
                    PkgRequest::from_ident(
                        embedded.ident().to_any(),
                        RequestedBy::Embedded(parent.clone()),
                    ),
                ))];
                changes.extend(self.set_package(
                    Arc::new(embedded.clone()),
                    PackageSource::Embedded {
                        parent: parent.clone(),
                    },
                ));
                changes
            })
            .collect()
    }

    fn options_to_change(spec: &Spec) -> Change {
        let mut opts = OptionMap::default();
        opts.insert(
            spec.name().as_opt_name().to_owned(),
            spec.compat().render(spec.version()).into(),
        );
        for (name, value) in spec.option_values() {
            if !value.is_empty() {
                let name = name.with_default_namespace(spec.name());
                opts.insert(name, value);
            }
        }
        Change::SetOptions(SetOptions::new(opts))
    }
}

#[derive(Clone, Debug, Diagnostic, Error)]
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
                    let first_change = decision.changes.first();

                    self.nodes.insert(new_node_lock.id(), new_node.clone());

                    let node_to_copy_iterators_from = match first_change {
                        // XXX: This matches the shape of the Decision
                        // produced by `reconsider_package` but a better way
                        // to cause this behavior is needed.
                        Some(Change::StepBack(step_back)) if decision.changes.len() == 2 => {
                            // Resume iterating packages at the point we're
                            // jumping back to so we don't skip builds that
                            // were visited between now and where we're going
                            // back to.
                            self.nodes
                                .get(&step_back.destination.id())
                                .expect("destination node exists")
                        }
                        _ => &old_node,
                    };

                    for (name, iterator) in
                        node_to_copy_iterators_from.read().await.iterators.iter()
                    {
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
                unsafe { decision.changes.first().unwrap_unchecked() },
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
        package_name: PkgNameBuf,
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
    pub request: PkgRequest,
    pub prioritize: bool,
}

/// For counting requests for the same package, but not necessarily
/// the same version, e.g. gcc/9 and gcc/6 are the same package.
pub static REQUESTS_FOR_SAME_PACKAGE_COUNT: AtomicU64 = AtomicU64::new(0);
/// For counting completely identical duplicate requests that appear,
/// e.g. gcc/~9.3 and gcc/~9.3, but not gcc/6
pub static DUPLICATE_REQUESTS_COUNT: AtomicU64 = AtomicU64::new(0);

impl RequestPackage {
    pub fn new(request: PkgRequest) -> Self {
        RequestPackage {
            request,
            prioritize: false,
        }
    }

    pub fn prioritize(request: PkgRequest) -> Self {
        RequestPackage {
            request,
            prioritize: true,
        }
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

            // Apply the configured request priority ordering to the request
            // list.
            REQUESTS_PRIORITY_ORDER
                .promote_names(new_requests.as_mut_slice(), |req| req.pkg.name().as_str());
        }

        if self.prioritize {
            // Move the request to the front of new_requests.
            //
            // This requires a stable sort.
            new_requests.sort_by_key(|req| i32::from(req.pkg.name != self.request.pkg.name))
        }

        Arc::new(base.with_pkg_requests(parent, new_requests))
    }
}

#[derive(Clone, Debug)]
pub struct RequestVar {
    pub request: VarRequest,
}

impl RequestVar {
    pub fn new(request: VarRequest) -> Self {
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
            base,
            vec![(
                &self.request.var,
                &self
                    .request
                    .value
                    .as_pinned()
                    .map(str::to_string)
                    .unwrap_or_default(),
            )]
            .into_iter(),
            true,
        );
        Arc::new(base.with_var_requests_and_options(parent, new_requests, options))
    }
}

#[derive(Clone, Debug)]
pub struct SetOptions {
    pub options: OptionMap,
}

impl SetOptions {
    pub fn new(options: OptionMap) -> Self {
        SetOptions { options }
    }

    pub fn apply(&self, parent: &Arc<State>, base: &Arc<State>) -> Arc<State> {
        let new_options = Self::compute_new_options(base, self.options.iter(), false);
        Arc::new(base.with_options(parent, new_options))
    }

    /// Compute the new options list for a state, preserving the insertion
    /// order based on option key.
    pub fn compute_new_options<'i, I, V>(
        base: &State,
        new_options: I,
        update_existing_option_with_empty_value: bool,
    ) -> BTreeMap<OptNameBuf, Arc<str>>
    where
        I: Iterator<Item = (&'i OptNameBuf, &'i V)>,
        V: AsRef<str> + Into<Arc<str>> + Clone + 'i,
    {
        let mut options = (*base.options).clone();
        // Update base options with request options...
        for (k, v) in new_options {
            match options.get_mut(k) {
                // Unless already present and request option value is empty.
                Some(_) if v.as_ref().is_empty() && !update_existing_option_with_empty_value => {
                    continue
                }
                // If option already existed, change the value
                Some(value) => *value = (*v).clone().into(),
                None => {
                    options.insert(k.to_owned(), (*v).clone().into());
                }
            };
        }
        options
    }
}

#[derive(Clone, Debug)]
pub struct SetPackage {
    pub spec: Arc<Spec>,
    pub source: PackageSource,
}

impl SetPackage {
    pub fn new(spec: Arc<Spec>, source: PackageSource) -> Self {
        SetPackage { spec, source }
    }

    pub fn apply(&self, parent: &Arc<State>, base: &Arc<State>) -> Arc<State> {
        Arc::new(base.append_package(Some(parent), self.spec.clone(), self.source.clone()))
    }
}

/// Sets a package in the resolve, denoting is as a new build.
#[derive(Clone, Debug)]
pub struct SetPackageBuild {
    pub spec: Arc<Spec>,
    pub source: PackageSource,
}

impl SetPackageBuild {
    pub fn new(spec: Arc<Spec>, recipe: Arc<SpecRecipe>) -> Self {
        SetPackageBuild {
            spec,
            source: PackageSource::BuildFromSource { recipe },
        }
    }

    pub fn apply(&self, parent: &Arc<State>, base: &Arc<State>) -> Arc<State> {
        Arc::new(base.append_package(Some(parent), self.spec.clone(), self.source.clone()))
    }
}

#[derive(Clone, Debug)]
pub struct StateId {
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
    pub fn id(&self) -> u64 {
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

    fn options_hash(options: &BTreeMap<OptNameBuf, Arc<str>>) -> u64 {
        let mut hasher = DefaultHasher::new();
        options.hash(&mut hasher);
        hasher.finish()
    }

    fn pkg_requests_hash(pkg_requests: &Vec<Arc<CachedHash<PkgRequest>>>) -> u64 {
        let mut hasher = DefaultHasher::new();
        pkg_requests.hash(&mut hasher);
        hasher.finish()
    }

    fn packages_hash(packages: &StatePackages) -> u64 {
        let mut hasher = DefaultHasher::new();
        for (spec, _, _) in packages.values() {
            spec.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn var_requests_hash(var_requests: &BTreeSet<VarRequest>) -> (u64, HashSet<u64>) {
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

    fn with_options(&self, options: &BTreeMap<OptNameBuf, Arc<str>>) -> Self {
        Self::new(
            self.pkg_requests_hash,
            self.var_requests_hash,
            Arc::clone(&self.var_requests_membership),
            self.packages_hash,
            StateId::options_hash(options),
        )
    }

    fn with_pkg_requests(&self, pkg_requests: &Vec<Arc<CachedHash<PkgRequest>>>) -> Self {
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
        var_requests: &BTreeSet<VarRequest>,
        options: &BTreeMap<OptNameBuf, Arc<str>>,
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

/// For caching the hash of an `PkgRequest`.
///
/// Computing the hash of `PkgRequest` represents a significant portion
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

type StatePackages = Arc<
    BTreeMap<
        PkgNameBuf,
        (
            CachedHash<Arc<Spec>>,
            PackageSource,
            // The state id just before this package was added to the state.
            StateId,
        ),
    >,
>;

// `State` is immutable. It should not derive Clone.
#[derive(Debug)]
pub struct State {
    pkg_requests: Arc<Vec<Arc<CachedHash<PkgRequest>>>>,
    var_requests: Arc<BTreeSet<VarRequest>>,
    packages: StatePackages,
    // A list of the packages in the order they were resolved and
    // added to the state. It differs from the "packages" field in
    // that it does not alphabetically order the packages and is less
    // efficient for processing. This field does not contribute to the
    // state id. It is used to track the resolve order for a solution.
    packages_in_solve_order: Arc<Vec<Arc<Spec>>>,
    options: Arc<BTreeMap<OptNameBuf, Arc<str>>>,
    state_id: StateId,
    cached_option_map: Arc<OnceCell<OptionMap>>,
    // How deep is this state?
    pub state_depth: u64,
    cached_unresolved_pkg_requests: Arc<OnceCell<HashMap<PkgNameBuf, PkgRequest>>>,
}

impl State {
    pub fn new(
        pkg_requests: Vec<PkgRequest>,
        var_requests: Vec<VarRequest>,
        packages: Vec<(Arc<Spec>, PackageSource)>,
        options: Vec<(OptNameBuf, Arc<str>)>,
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
            packages_in_solve_order: Arc::new(Vec::new()),
            options: Arc::new(options),
            state_id,
            cached_option_map: Arc::new(OnceCell::new()),
            state_depth: 0,
            cached_unresolved_pkg_requests: Arc::new(OnceCell::new()),
        };
        for (package, source) in packages.into_iter() {
            s = s.append_package(None, package, source)
        }
        Arc::new(s)
    }

    pub fn as_solution(&self) -> Result<Solution> {
        let mut solution = Solution::new((&self.options).into());

        // Ensure the resolved packages are added to the solution in
        // resolve order to preserve that order in the solution.
        for package in self.packages_in_solve_order.iter() {
            let (spec, source) = match self.packages.get(package.name()) {
                Some((pkg_spec, pkg_source, _)) => (pkg_spec, pkg_source),
                None => continue,
            };

            let req = self
                .get_merged_request(spec.name())
                .map_err(GraphError::RequestError)?;
            solution.add(req, Arc::clone(spec), source.clone());
        }
        Ok(solution)
    }

    /// Return true if this state already contains this request.
    pub fn contains_var_request(&self, var_request: &VarRequest) -> bool {
        let mut hasher = DefaultHasher::new();
        var_request.hash(&mut hasher);
        let single_var_request_hash = hasher.finish();
        self.state_id
            .var_requests_membership
            .contains(&single_var_request_hash)
    }

    pub fn default_state() -> Arc<Self> {
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
    ) -> super::error::GetCurrentResolveResult<(&CachedHash<Arc<Spec>>, &PackageSource, &StateId)>
    {
        // this lint is a false-positive as we are converting &(...) into (&...)
        #[allow(clippy::map_identity)]
        self.packages
            .get(name)
            .map(|(s, p, id)| (s, p, id))
            .ok_or_else(|| {
                super::error::GetCurrentResolveError::PackageNotResolved(format!(
                    "Has not been resolved: '{name}'",
                ))
            })
    }

    pub fn get_merged_request(
        &self,
        name: &PkgName,
    ) -> super::error::GetMergedRequestResult<PkgRequest> {
        // tests reveal this method is not safe to cache.
        let mut merged: Option<PkgRequest> = None;
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
                    merged.restrict(request).map_err(crate::Error::from)?;
                }
            }
        }
        match merged {
            Some(merged) => Ok(merged),
            None => Err(super::error::GetMergedRequestError::NoRequestFor(format!(
                "No requests for '{name}' [INTERNAL ERROR]"
            ))),
        }
    }

    pub fn get_next_request(&self) -> Result<Option<PkgRequest>> {
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

    pub fn get_pkg_requests(&self) -> &Vec<Arc<CachedHash<PkgRequest>>> {
        &self.pkg_requests
    }

    pub fn get_var_requests(&self) -> &BTreeSet<VarRequest> {
        &self.var_requests
    }

    /// Get a mapping of pkg name -> merged request for the unresolved
    /// PkgRequests in this state
    pub fn get_unresolved_requests(&self) -> &HashMap<PkgNameBuf, PkgRequest> {
        self.cached_unresolved_pkg_requests.get_or_init(|| {
            let mut unresolved: HashMap<PkgNameBuf, PkgRequest> = HashMap::new();

            for req in self.pkg_requests.iter() {
                if unresolved.contains_key(&req.pkg.name) {
                    continue;
                }
                if self.get_current_resolve(&req.pkg.name).is_err() {
                    unresolved.insert(
                        req.pkg.name.clone(),
                        self.get_merged_request(&req.pkg.name).unwrap(),
                    );
                }
            }
            unresolved
        })
    }

    pub fn get_ordered_resolved_packages(&self) -> &Arc<Vec<Arc<Spec>>> {
        &self.packages_in_solve_order
    }

    pub fn get_resolved_packages(
        &self,
    ) -> &BTreeMap<PkgNameBuf, (CachedHash<Arc<Spec>>, PackageSource, StateId)> {
        &self.packages
    }

    #[inline]
    pub fn get_resolved_packages_hash(&self) -> u64 {
        self.state_id.packages_hash
    }

    fn with_options(&self, parent: &Self, options: BTreeMap<OptNameBuf, Arc<str>>) -> Self {
        let state_id = self.state_id.with_options(&options);
        Self {
            pkg_requests: Arc::clone(&self.pkg_requests),
            var_requests: Arc::clone(&self.var_requests),
            packages: Arc::clone(&self.packages),
            packages_in_solve_order: Arc::clone(&self.packages_in_solve_order),
            options: Arc::new(options),
            state_id,
            // options are changing
            cached_option_map: Arc::new(OnceCell::new()),
            state_depth: parent.state_depth + 1,
            // unresolved pkg requests are the same
            cached_unresolved_pkg_requests: Arc::clone(&self.cached_unresolved_pkg_requests),
        }
    }

    fn append_package(
        &self,
        parent: Option<&Arc<Self>>,
        spec: Arc<Spec>,
        source: PackageSource,
    ) -> Self {
        let mut packages_in_solve_order = Arc::clone(&self.packages_in_solve_order);
        Arc::make_mut(&mut packages_in_solve_order).push(Arc::clone(&spec));
        let mut packages = Arc::clone(&self.packages);
        Arc::make_mut(&mut packages).insert(
            spec.name().to_owned(),
            (spec.into(), source, self.state_id.clone()),
        );
        let state_id = self.state_id.with_packages(&packages);
        Self {
            pkg_requests: Arc::clone(&self.pkg_requests),
            var_requests: Arc::clone(&self.var_requests),
            packages,
            packages_in_solve_order,
            options: Arc::clone(&self.options),
            state_id,
            // options are the same
            cached_option_map: Arc::clone(&self.cached_option_map),
            state_depth: parent.as_ref().map(|p| p.state_depth + 1).unwrap_or(0),
            // unresolved pkg requests change because a package was resolved
            cached_unresolved_pkg_requests: Arc::new(OnceCell::new()),
        }
    }

    fn with_pkg_requests(
        &self,
        parent: &Self,
        pkg_requests: Vec<Arc<CachedHash<PkgRequest>>>,
    ) -> Self {
        let state_id = self.state_id.with_pkg_requests(&pkg_requests);
        Self {
            pkg_requests: Arc::new(pkg_requests),
            var_requests: Arc::clone(&self.var_requests),
            packages: Arc::clone(&self.packages),
            packages_in_solve_order: Arc::clone(&self.packages_in_solve_order),
            options: Arc::clone(&self.options),
            state_id,
            // options are the same
            cached_option_map: Arc::clone(&self.cached_option_map),
            state_depth: parent.state_depth + 1,
            // unresolved pkg requests (may) change because a new request was added
            cached_unresolved_pkg_requests: Arc::new(OnceCell::new()),
        }
    }

    fn with_var_requests_and_options(
        &self,
        parent: &Self,
        var_requests: Arc<BTreeSet<VarRequest>>,
        options: BTreeMap<OptNameBuf, Arc<str>>,
    ) -> Self {
        let state_id = self
            .state_id
            .with_var_requests_and_options(&var_requests, &options);
        Self {
            pkg_requests: Arc::clone(&self.pkg_requests),
            var_requests,
            packages: Arc::clone(&self.packages),
            packages_in_solve_order: Arc::clone(&self.packages_in_solve_order),
            options: Arc::new(options),
            state_id,
            // options are changing
            cached_option_map: Arc::new(OnceCell::new()),
            state_depth: parent.state_depth + 1,
            // unresolved pkg requests are the same
            cached_unresolved_pkg_requests: Arc::clone(&self.cached_unresolved_pkg_requests),
        }
    }

    pub fn get_option_map(&self) -> &OptionMap {
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
    Compatibility(Compatibility),
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
    pub pkg: AnyIdent,
    pub reason: SkipPackageNoteReason,
}

impl SkipPackageNote {
    pub fn new(pkg: AnyIdent, reason: Compatibility) -> Self {
        SkipPackageNote {
            pkg,
            reason: SkipPackageNoteReason::Compatibility(reason),
        }
    }

    pub fn new_from_message<S: ToString>(pkg: AnyIdent, reason: S) -> Self {
        SkipPackageNote {
            pkg,
            reason: SkipPackageNoteReason::String(reason.to_string()),
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
