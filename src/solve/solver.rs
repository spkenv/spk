// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    borrow::Cow,
    collections::HashMap,
    mem::take,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use async_stream::stream;
use futures::{Stream, TryStreamExt};
use nonempty::{nonempty, NonEmpty};
use priority_queue::priority_queue::PriorityQueue;

use crate::{
    api::{self, Build, Component, OptionMap, PkgName, PkgRequest, Request},
    solve::graph::StepBack,
    storage, Error, Result,
};

use super::{
    errors,
    graph::{
        self, Change, Decision, Graph, Node, Note, RequestPackage, RequestVar, SkipPackageNote,
        State, DEAD_STATE,
    },
    package_iterator::{
        BuildIterator, EmptyBuildIterator, PackageIterator, RepositoryPackageIterator,
        SortedBuildIterator,
    },
    solution::{PackageSource, Solution},
    validation::{self, BinaryOnlyValidator, ValidatorT, Validators},
};

// Public to allow other tests to use its macros
#[cfg(test)]
#[path = "./solver_test.rs"]
mod solver_test;

/// Possible outcomes from [`Solver::step_state`]
#[derive(Debug)]
enum StepStateOutcome {
    SolveComplete,
    Decisions(Decisions),
}

impl From<StepStateOutcome> for Option<Decisions> {
    fn from(outcome: StepStateOutcome) -> Self {
        match outcome {
            StepStateOutcome::SolveComplete => None,
            StepStateOutcome::Decisions(decisions) => Some(decisions),
        }
    }
}

#[derive(Clone)]
pub struct Solver {
    repos: Vec<Arc<storage::RepositoryHandle>>,
    initial_state_builders: Vec<Change>,
    validators: Cow<'static, [Validators]>,
    // For counting the number of steps (forward) taken in a solve
    number_of_steps: usize,
    // For counting number of builds skipped for some reason
    number_builds_skipped: usize,
    // For counting the number of incompatible versions
    number_incompat_versions: usize,
    // For counting the number of incompatible builds
    number_incompat_builds: usize,
    // For counting the total number of builds expanded so far in
    // the solve
    number_total_builds: usize,
    // For counting the number of StepBacks applied during the solve
    number_of_steps_back: Arc<AtomicU64>,
    // For accumulating the frequency of error messages generated
    // during the solver. Used in end-of-solve stats or if the solve
    // is interrupted by the user or timeout.
    error_frequency: HashMap<String, u64>,
    // For counting the number of times packages are involved in
    // blocked requests for other packages during the search. Used to
    // highlight problem areas in a solve and help user home in on
    // what might be causing issues.
    problem_packages: HashMap<String, u64>,
}

impl Default for Solver {
    fn default() -> Self {
        Self {
            repos: Vec::default(),
            initial_state_builders: Vec::default(),
            validators: Cow::from(validation::default_validators()),
            number_of_steps: 0,
            number_builds_skipped: 0,
            number_incompat_versions: 0,
            number_incompat_builds: 0,
            number_total_builds: 0,
            number_of_steps_back: Arc::new(AtomicU64::new(0)),
            error_frequency: HashMap::new(),
            problem_packages: HashMap::new(),
        }
    }
}

impl Solver {
    /// Add a request to this solver.
    pub fn add_request(&mut self, request: api::Request) {
        let request = match request {
            Request::Pkg(mut request) => {
                if request.pkg.components.is_empty() {
                    request
                        .pkg
                        .components
                        .insert(api::Component::default_for_run());
                }
                Change::RequestPackage(RequestPackage::new(request))
            }
            Request::Var(request) => Change::RequestVar(RequestVar::new(request)),
        };
        self.initial_state_builders.push(request);
    }

    /// Add a repository where the solver can get packages.
    pub fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<storage::RepositoryHandle>>,
    {
        self.repos.push(repo.into());
    }

    pub fn get_initial_state(&self) -> Arc<State> {
        let mut state = None;
        let base = State::default();
        for change in self.initial_state_builders.iter() {
            state = Some(change.apply(&base, state.as_ref().unwrap_or(&base)));
        }
        state.unwrap_or(base)
    }

    /// Increment the number of occurrences of the given error message
    pub fn increment_error_count(&mut self, error_message: String) {
        let counter = self.error_frequency.entry(error_message).or_insert(0);
        *counter += 1;
    }

    /// Get the error to frequency mapping
    pub fn error_frequency(&self) -> &HashMap<String, u64> {
        &self.error_frequency
    }

    /// Increment the number of occurrences of the given error message
    pub fn increment_problem_package_count(&mut self, problem_package: String) {
        let counter = self.problem_packages.entry(problem_package).or_insert(0);
        *counter += 1;
    }

    /// Get the problem packages frequency mapping
    pub fn problem_packages(&self) -> &HashMap<String, u64> {
        &self.problem_packages
    }

    async fn get_iterator(
        &self,
        node: &mut Arc<Node>,
        package_name: &PkgName,
    ) -> Arc<tokio::sync::Mutex<Box<dyn PackageIterator + Send>>> {
        if let Some(iterator) = node.get_iterator(package_name) {
            return iterator;
        }
        let iterator = self.make_iterator(package_name.to_owned());
        Arc::make_mut(node)
            .set_iterator(package_name.to_owned(), &iterator)
            .await;
        iterator
    }

    fn make_iterator(
        &self,
        package_name: api::PkgNameBuf,
    ) -> Arc<tokio::sync::Mutex<Box<dyn PackageIterator + Send>>> {
        debug_assert!(!self.repos.is_empty());
        Arc::new(tokio::sync::Mutex::new(Box::new(
            RepositoryPackageIterator::new(package_name, self.repos.clone()),
        )))
    }

    #[async_recursion::async_recursion]
    async fn resolve_new_build(&self, spec: &api::Spec, state: &State) -> Result<Solution> {
        let mut opts = state.get_option_map().clone();
        for pkg_request in state.get_pkg_requests() {
            if !opts.contains_key(pkg_request.pkg.name.as_opt_name()) {
                opts.insert(
                    pkg_request.pkg.name.as_opt_name().to_owned(),
                    pkg_request.pkg.version.to_string(),
                );
            }
        }
        for var_request in state.get_var_requests() {
            if !opts.contains_key(&var_request.var) {
                opts.insert(var_request.var.clone(), var_request.value.clone());
            }
        }

        let mut solver = Solver {
            repos: self.repos.clone(),
            ..Default::default()
        };
        solver.update_options(opts.clone());
        solver.solve_build_environment(spec).await
    }

    async fn step_builds<F>(
        &mut self,
        node: &Node,
        request: &PkgRequest,
        builds: &Arc<tokio::sync::Mutex<F>>,
        notes: &mut Vec<Note>,
    ) -> Result<Option<Decision>>
    where
        F: BuildIterator + ?Sized,
    {
        while let Some((mut spec, repo)) = builds.lock().await.next().await? {
            // Now all this build to the total considered during
            // this overall step
            self.number_total_builds += 1;

            let build_from_source =
                spec.pkg.build == Some(Build::Source) && request.pkg.build != Some(Build::Source);
            if build_from_source {
                if let PackageSource::Spec(spec) = repo {
                    notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                        spec.pkg.clone(),
                        "cannot build embedded source package",
                    )));
                    self.number_builds_skipped += 1;
                    continue;
                }

                match repo.read_spec(&spec.pkg.with_build(None)).await {
                    Ok(s) => spec = s,
                    Err(Error::PackageNotFoundError(pkg)) => {
                        notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                            pkg,
                            "cannot build from source, version spec not available",
                        )));
                        self.number_builds_skipped += 1;
                        continue;
                    }
                    Err(err) => return Err(err),
                }
            }

            let compat = self.validate(&node.state, &spec, &repo)?;
            if !&compat {
                notes.push(Note::SkipPackageNote(SkipPackageNote::new(
                    spec.pkg.clone(),
                    compat,
                )));
                self.number_builds_skipped += 1;
                continue;
            }

            let mut decision = if build_from_source {
                match self.resolve_new_build(&spec, &node.state).await {
                    Ok(build_env) => {
                        match Decision::builder(spec.clone(), &node.state)
                            .with_components(&request.pkg.components)
                            .build_package(&build_env)
                        {
                            Ok(decision) => decision,
                            Err(err) => {
                                notes.push(Note::SkipPackageNote(
                                    SkipPackageNote::new_from_message(
                                        spec.pkg.clone(),
                                        &format!("cannot build package: {:?}", err),
                                    ),
                                ));
                                self.number_builds_skipped += 1;
                                continue;
                            }
                        }
                    }

                    // FIXME: This should only match `SolverError`
                    Err(err) => {
                        notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                            spec.pkg.clone(),
                            &format!("cannot resolve build env: {:?}", err),
                        )));
                        self.number_builds_skipped += 1;
                        continue;
                    }
                }
            } else {
                Decision::builder(spec, &node.state)
                    .with_components(&request.pkg.components)
                    .resolve_package(repo)
            };
            decision.add_notes(notes.iter().cloned());
            return Ok(Some(decision));
        }
        Ok(None)
    }

    /// Find the next thing to try adding to the graph.
    ///
    /// 1. Find the next package waiting to be solved.
    /// 2. Each call to `step_state` looks at a single version of that package.
    /// 3. For each version, return a list of all the viable builds. If there
    ///    are no viable builds, an error is returned.
    async fn step_state(&mut self, node: &mut Arc<Node>) -> Result<StepStateOutcome> {
        let mut notes = Vec::<Note>::new();
        let request = if let Some(request) = node.state.get_next_request()? {
            request
        } else {
            return Ok(StepStateOutcome::SolveComplete);
        };

        // This is a step forward in the solve
        self.number_of_steps += 1;

        let iterator = self.get_iterator(node, &request.pkg.name).await;
        let mut iterator_lock = iterator.lock().await;
        loop {
            let (pkg, builds) = match iterator_lock.next().await {
                Ok(Some((pkg, builds))) => (pkg, builds),
                Ok(None) => break,
                Err(Error::PackageNotFoundError(_)) => {
                    // Intercept this error in this situation to
                    // capture the request for the package that turned
                    // out to be missing.
                    return Err(Error::Solve(errors::Error::PackageNotFoundDuringSolve(
                        request.clone(),
                    )));
                }
                Err(e) => return Err(e),
            };

            let compat = request.is_version_applicable(&pkg.version);
            if !&compat {
                // Count this version and its builds as incompatible
                self.number_incompat_versions += 1;
                self.number_incompat_builds += builds.lock().await.len();

                // Skip this version and move on the to next one
                iterator_lock.set_builds(
                    &pkg.version,
                    Arc::new(tokio::sync::Mutex::new(EmptyBuildIterator::new())),
                );
                notes.push(Note::SkipPackageNote(SkipPackageNote::new(
                    pkg.clone(),
                    compat,
                )));
                continue;
            }

            let builds = if !builds.lock().await.is_sorted_build_iterator() {
                let builds = Arc::new(tokio::sync::Mutex::new(
                    SortedBuildIterator::new(node.state.get_option_map().clone(), builds.clone())
                        .await?,
                ));
                iterator_lock.set_builds(&pkg.version, builds.clone());
                builds
            } else {
                builds
            };

            let mut decisions = Vec::new();

            loop {
                match self.step_builds(node, &request, &builds, &mut notes).await {
                    Ok(None) => break,
                    Ok(Some(decision)) => decisions.push(Arc::new(decision)),
                    Err(err) => return Err(err),
                }
            }

            // Safety: removing this line would violate safety requirements
            // below.
            if decisions.is_empty() {
                continue;
            }

            return Ok(StepStateOutcome::Decisions(
                // Safety: Already confirmed that `decisions` is not empty.
                unsafe { NonEmpty::from_vec(decisions).unwrap_unchecked() },
            ));
        }

        Err(errors::Error::OutOfOptions(errors::OutOfOptions { request, notes }).into())
    }

    fn validate(
        &self,
        node: &State,
        spec: &api::Spec,
        source: &PackageSource,
    ) -> Result<api::Compatibility> {
        for validator in self.validators.as_ref() {
            let compat = validator.validate(node, spec, source)?;
            if !&compat {
                return Ok(compat);
            }
        }
        Ok(api::Compatibility::Compatible)
    }

    /// Put this solver back into its default state
    pub fn reset(&mut self) {
        self.repos.truncate(0);
        self.initial_state_builders.truncate(0);
        self.validators = Cow::from(validation::default_validators());
        self.number_of_steps = 0;
        self.number_builds_skipped = 0;
        self.number_incompat_versions = 0;
        self.number_incompat_builds = 0;
        self.number_total_builds = 0;
        self.number_of_steps_back.store(0, Ordering::SeqCst);
        self.error_frequency.clear();
        self.problem_packages.clear();
    }

    /// Run this solver
    pub fn run(&self) -> SolverRuntime {
        SolverRuntime::new(self.clone())
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
                .to_mut()
                .insert(0, Validators::BinaryOnly(BinaryOnlyValidator {}))
        } else {
            // Remove all BinaryOnly validators because one was found.
            self.validators = take(self.validators.to_mut())
                .into_iter()
                .filter(|v| !matches!(v, Validators::BinaryOnly(_)))
                .collect();
        }
    }

    pub async fn solve(&mut self) -> Result<Solution> {
        let mut runtime = self.run();
        {
            let iter = runtime.iter();
            tokio::pin!(iter);
            while let Some(_step) = iter.try_next().await? {}
        }
        runtime.current_solution().await
    }

    /// Adds requests for all build requirements
    pub fn configure_for_build_environment(&mut self, spec: &api::Spec) -> Result<()> {
        let state = self.get_initial_state();

        let build_options = spec.resolve_all_options(state.get_option_map());
        for option in &spec.build.options {
            if let api::Opt::Pkg(option) = option {
                let given = build_options.get(option.pkg.as_opt_name());

                let mut request = option.to_request(
                    given.cloned(),
                    api::RequestedBy::PackageBuild(spec.pkg.clone()),
                )?;
                // if no components were explicitly requested in a build option,
                // then we inject the default for this context
                if request.pkg.components.is_empty() {
                    request
                        .pkg
                        .components
                        .insert(Component::default_for_build());
                }
                self.add_request(request.into())
            }
        }

        Ok(())
    }

    /// Adds requests for all build requirements and solves
    pub async fn solve_build_environment(&mut self, spec: &api::Spec) -> Result<Solution> {
        self.configure_for_build_environment(spec)?;
        self.solve().await
    }

    pub fn update_options(&mut self, options: OptionMap) {
        self.initial_state_builders
            .push(Change::SetOptions(graph::SetOptions::new(options)))
    }

    /// Get the number of steps (forward) taken in the solve
    pub fn get_number_of_steps(&self) -> usize {
        self.number_of_steps
    }

    /// Get the number of builds skipped during the solve
    pub fn get_number_of_builds_skipped(&self) -> usize {
        self.number_builds_skipped
    }

    /// Get the number of incompatible versions avoided during the solve
    pub fn get_number_of_incompatible_versions(&self) -> usize {
        self.number_incompat_versions
    }

    /// Get the number of incompatible builds avoided during the solve
    pub fn get_number_of_incompatible_builds(&self) -> usize {
        self.number_incompat_builds
    }

    /// Get the total number of builds examined during the solve
    pub fn get_total_builds(&self) -> usize {
        self.number_total_builds
    }

    /// Get the number of steps back taken during the solve
    pub fn get_number_of_steps_back(&self) -> u64 {
        self.number_of_steps_back.load(Ordering::SeqCst)
    }
}

pub(crate) type RwNode = Arc<tokio::sync::RwLock<Arc<Node>>>;

// This is needed so `PriorityQueue` doesn't need to hash the node itself.
#[derive(Clone, Debug)]
struct NodeWrapper {
    pub(crate) node: RwNode,
    pub(crate) hash: u64,
}

impl std::cmp::Eq for NodeWrapper {}

impl std::cmp::PartialEq for NodeWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl std::hash::Hash for NodeWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

type Nodes = NonEmpty<NodeWrapper>;
type Decisions = NonEmpty<Arc<Decision>>;

type SolverHistory = PriorityQueue<Nodes, std::cmp::Reverse<u64>>;

/// A [`Decision`] that was derived from a [`Node`].
#[derive(Debug)]
struct DecisionFromNode {
    pub(crate) decision: Arc<Decision>,
    pub(crate) node: RwNode,
}

/// The possible states that a solver's current decisions can be in.
#[derive(Debug, Default)]
enum SolverDecisions {
    Initial(Arc<Decision>),
    DecisionsFromNode(NonEmpty<DecisionFromNode>),
    SolveComplete,
    #[default]
    Empty,
}

impl SolverDecisions {
    /// Return if the solver has completed.
    #[inline]
    pub(crate) fn is_complete(&self) -> bool {
        matches!(self, SolverDecisions::SolveComplete)
    }

    /// Extract the decisions, if any, and leave [`SolverDecisions::empty`]
    /// in its place.
    pub(crate) fn take(&mut self) -> Option<NonEmpty<(Arc<Decision>, Option<RwNode>)>> {
        match self {
            SolverDecisions::SolveComplete => return None,
            SolverDecisions::Empty => return None,
            _ => {}
        };

        match std::mem::take(self) {
            SolverDecisions::Initial(decision) => Some(nonempty![(decision, None)]),
            SolverDecisions::DecisionsFromNode(decisions) => {
                Some(decisions.map(|d| (d.decision, Some(d.node))))
            }
            _ => unreachable!(),
        }
    }
}

#[must_use = "The solver runtime does nothing unless iterated to completion"]
pub struct SolverRuntime {
    pub solver: Solver,
    graph: Arc<tokio::sync::RwLock<Graph>>,
    history: SolverHistory,
    current_nodes: Option<Nodes>,
    decisions: SolverDecisions,
}

impl SolverRuntime {
    pub fn new(solver: Solver) -> Self {
        let initial_decision = Decision::new(solver.initial_state_builders.clone());
        Self {
            solver,
            graph: Arc::new(tokio::sync::RwLock::new(Graph::new())),
            history: SolverHistory::default(),
            current_nodes: None,
            decisions: SolverDecisions::Initial(Arc::new(initial_decision)),
        }
    }

    /// A reference to the solve graph being built by this runtime
    pub fn graph(&self) -> Arc<tokio::sync::RwLock<Graph>> {
        self.graph.clone()
    }

    /// Returns the completed solution for this runtime.
    ///
    /// If needed, this function will iterate any remaining
    /// steps for the current state.
    pub async fn solution(&mut self) -> Result<Solution> {
        {
            let iter = self.iter();
            tokio::pin!(iter);
            while let Some(_item) = iter.try_next().await? {}
        }
        self.current_solution().await
    }

    /// Return the current solution for this runtime.
    ///
    /// If the runtime has not yet completed, this solution
    /// may be incomplete or empty.
    pub async fn current_solution(&self) -> Result<Solution> {
        let current_node = &self
            .current_nodes
            .as_ref()
            .ok_or_else(|| Error::String("Solver runtime has not been consumed".into()))?
            .head;
        let current_node_lock = current_node.node.read().await;

        let is_dead = current_node_lock.state.id()
            == self.graph.read().await.root.read().await.state.id()
            || Arc::ptr_eq(&current_node_lock.state, &DEAD_STATE);
        let is_empty = self
            .solver
            .get_initial_state()
            .get_pkg_requests()
            .is_empty();
        if is_dead && !is_empty {
            Err(super::Error::FailedToResolve((*self.graph).read().await.clone()).into())
        } else {
            current_node_lock
                .state
                .as_solution()
                // TODO: make a proper conversion for this
                .map_err(|e| crate::Error::String(e.to_string()))
        }
    }

    // TODO: turn this into an instance method, and rework the
    // borrowing in next() to allow fewer parameters to be passed into
    // this method.
    /// Generate step-back decision from a node history
    async fn take_a_step_back(
        history: &mut SolverHistory,
        decisions: &mut SolverDecisions,
        solver: &Solver,
        message: &String,
    ) {
        // After encountering a solver error, start trying a new path from the
        // oldest fork. Experimentation shows that this is able to discover
        // a valid solution must faster than going back to the newest fork,
        // for problem cases that get stuck in a bad path.
        match history.pop() {
            Some((n, _)) => {
                let mut new_decisions = Vec::with_capacity(n.len());
                for node in n.into_iter() {
                    let n_lock = node.node.read().await;
                    let to = Arc::clone(&n_lock.state);
                    drop(n_lock);
                    new_decisions.push(DecisionFromNode {
                        decision: Arc::new(
                            Change::StepBack(StepBack::new(
                                message,
                                to,
                                Arc::clone(&solver.number_of_steps_back),
                            ))
                            .as_decision(),
                        ),
                        node: node.node,
                    });
                }

                *decisions = SolverDecisions::DecisionsFromNode(
                    // Safety: `n` was `NonEmpty`.
                    unsafe { NonEmpty::from_vec(new_decisions).unwrap_unchecked() },
                )
            }
            None => {
                *decisions = SolverDecisions::Initial(Arc::new(
                    Change::StepBack(StepBack::new(
                        message,
                        Arc::clone(&DEAD_STATE),
                        Arc::clone(&solver.number_of_steps_back),
                    ))
                    .as_decision(),
                ))
            }
        }
    }

    /// Iterate through each step of this runtime, trying to converge on a solution
    pub fn iter(&mut self) -> impl Stream<Item = Result<(Arc<Node>, Arc<Decision>)>> + Send + '_ {
        stream! {
            'outer: loop {
                if self.decisions.is_complete()
                    || (self.current_nodes.is_some()
                        && {
                            if let Some(n) = self.current_nodes.as_ref() {
                                Arc::ptr_eq(&n.head.node.read().await.state, &DEAD_STATE)
                            }
                            else {
                                false
                            }
                        })
                {
                    break 'outer;
                }

                let node_to_yield =
                    // A clone of Some(current_node) or the root node
                    {
                        if let Some(n) = self.current_nodes.as_ref() {
                            n.head.node.read().await.clone()
                        }
                        else {
                            self.graph.read().await.root.read().await.clone()
                        }
                    };

                let decisions_and_opt_node = self.decisions.take().unwrap();

                // Possible paths to take
                let mut branches = Vec::with_capacity(decisions_and_opt_node.len());
                // Paths that lead to ruin
                let mut failures = Vec::with_capacity(decisions_and_opt_node.len());

                {
                    let mut sg = self.graph.write().await;
                    let root_id = sg.root.read().await.id();
                    for (decision, opt_node) in decisions_and_opt_node.into_iter() {
                        match sg.add_branch({
                                if let Some(n) = opt_node.as_ref() {
                                    n.read().await.id()
                                }
                                else {
                                    root_id
                                }},
                                decision.clone(),
                            ).await {
                            Ok(cn) => branches.push((cn, decision)),
                            Err(err) => {
                                failures.push((dbg!(err), dbg!(decision)));
                            }
                        }
                    }
                }

                dbg!(branches.len());
                dbg!(failures.len());

                if branches.is_empty() {
                    SolverRuntime::take_a_step_back(
                        &mut self.history,
                        &mut self.decisions,
                        &self.solver,
                        // Safety: `self.decisions` is `NonEmpty` and the loop
                        // with `add_branch` must loop at least once and
                        // either add an element to `branches` or `failures`.
                        // Therefore, `failures` is non-empty.
                        &unsafe { failures.iter().next().unwrap_unchecked() }.0.to_string(),
                    ).await;
                    yield Ok((
                        node_to_yield,
                        // Safety: same argument as above.
                        unsafe { failures.into_iter().next().unwrap_unchecked() }.1
                    ));
                    continue 'outer;
                }

                // These are all the nodes we are exploring now.
                self.current_nodes = NonEmpty::from_vec(branches.iter().map(|(n, _)| NodeWrapper {
                    node: Arc::clone(n),
                    // I tried reversing the order of the hash here and it
                    // produced the same result.
                    hash: self.solver.get_number_of_steps() as u64,
                }).collect::<Vec<_>>());
                let mut current_level = 0;

                let mut sub_decisions = Vec::with_capacity(branches.len());
                let mut out_of_options = Vec::with_capacity(branches.len());
                let mut package_not_found = Vec::with_capacity(branches.len());

                for (current_node, decision) in branches.into_iter() {
                    // TODO: run this as a spawned task!
                    let mut current_node_lock = current_node.write().await;
                    // XXX: will all the nodes be on the same level?
                    current_level = current_node_lock.state.state_depth;
                    let outcome = self.solver.step_state(&mut current_node_lock).await;

                    match outcome
                    {
                        Ok(StepStateOutcome::SolveComplete) => {
                            self.decisions = SolverDecisions::SolveComplete;
                            yield Ok((node_to_yield, decision));
                            continue 'outer;
                        }
                        Ok(StepStateOutcome::Decisions(decisions)) => {
                            dbg!(decisions.len());
                            sub_decisions.push((decisions, Arc::clone(&current_node)));
                        }
                        Err(crate::Error::Solve(errors::Error::OutOfOptions(err))) => {
                            // Add to problem package counts based on what made
                            // the request for the blocked package.
                            let requested_by = err.request.get_requesters();
                            for req in &requested_by {
                                if let api::RequestedBy::PackageBuild(problem_package) = req {
                                    self.solver
                                        .increment_problem_package_count(problem_package.name.to_string())
                                }
                            }

                            // Add the requirers to the output so where the
                            // requests came from is more visible to the user.
                            let requirers: Vec<String> = requested_by.iter().map(ToString::to_string).collect();
                            let cause = format!(
                                "could not satisfy '{}' as required by: {}",
                                err.request.pkg,
                                requirers.join(", ")
                            );
                            self.solver.increment_error_count(cause.clone());

                            out_of_options.push((decision, err, cause));
                        }
                        Err(crate::Error::Solve(errors::Error::PackageNotFoundDuringSolve(err_req))) => {
                            let requested_by = err_req.get_requesters();
                            for req in &requested_by {
                                // Can't recover from a command line request for a
                                // missing package.
                                if let api::RequestedBy::CommandLine = req {
                                    yield Err(crate::Error::Solve(
                                        errors::Error::PackageNotFoundDuringSolve(err_req.clone()),
                                    ));
                                    continue 'outer;
                                }

                                // Add to problem package counts based on what
                                // made the request for the blocked package.
                                if let api::RequestedBy::PackageBuild(problem_package) = req {
                                    self.solver
                                        .increment_problem_package_count(problem_package.name.to_string())
                                };
                            }

                            let requirers: Vec<String> = requested_by.iter().map(ToString::to_string).collect();
                            let cause = format!("Package '{}' not found during the solve as required by: {}. Please check the spelling of the package's name", err_req.pkg, requirers.join(", "));

                            self.solver.increment_error_count(cause.clone());

                            package_not_found.push((decision, cause));
                        }
                        Err(err) => {
                            let cause = format!("{}", err);
                            self.solver.increment_error_count(cause);
                            yield Err(err);
                            continue 'outer;
                        }
                    };
                }

                dbg!(sub_decisions.len());
                dbg!(out_of_options.len());
                dbg!(package_not_found.len());

                if !sub_decisions.is_empty() {
                    // XXX: Does it make sense to yield all these decisions?
                    for (decisions, _node) in sub_decisions.iter() {
                        for decision in decisions.iter() {
                            // XXX: really yield `node_to_yield` here and not
                            // `_node`?
                            yield Ok((node_to_yield.clone(), decision.clone()))
                        }
                    }

                    // These branches of the search are still making progress.
                    // Make them our current set of decisions.
                    self.decisions = SolverDecisions::DecisionsFromNode(
                        // Safety: `sub_decisions` is already proven not empty.
                        unsafe {
                            NonEmpty::from_vec(
                                sub_decisions.into_iter().flat_map(
                                    |(decision, node)| decision.into_iter().map(
                                        move |decision| DecisionFromNode { decision, node: Arc::clone(&node) }
                                    )
                                )
                                .collect()
                            ).unwrap_unchecked()
                        }
                    );

                    self.history.push(
                        self.current_nodes.as_ref().expect("current_nodes always `is_some` here").clone(),
                        std::cmp::Reverse(current_level),
                    );

                    // XXX: What if there were _also_ errors?
                }
                else if !out_of_options.is_empty() {
                    debug_assert_eq!(
                        out_of_options.len(),
                        1,
                        "what does it mean for there to be more than one out of option error?"
                    );

                    for (mut decision, err, cause) in out_of_options.into_iter() {
                        SolverRuntime::take_a_step_back(
                            &mut self.history,
                            &mut self.decisions,
                            &self.solver,
                            &cause,
                        ).await;

                        Arc::make_mut(&mut decision).add_notes(err.notes.iter().cloned());

                        yield Ok((node_to_yield, decision));
                        continue 'outer;
                    }

                    // XXX: What if there were _also_ package not found errors?
                }
                else if !package_not_found.is_empty() {
                    debug_assert_eq!(
                        package_not_found.len(),
                        1,
                        "what does it mean for there to be more than one package not found error?"
                    );

                    for (decision, cause) in package_not_found.into_iter() {
                        SolverRuntime::take_a_step_back(
                            &mut self.history,
                            &mut self.decisions,
                            &self.solver,
                            &cause,
                        ).await;

                        // This doesn't halt the solve because the missing
                        // package might only have been requested by one
                        // build. It may not be required in the next build the
                        // solver looks at. However, this kind of error
                        // usually occurs either because of command line
                        // request that was mistyped, or because a requirement
                        // was misspelt in a yaml file that was given on a
                        // command line. So the solver is likely to hit a dead
                        // end and halt fairly soon.
                        yield Ok((node_to_yield.clone(), decision));
                        continue 'outer;
                    }
                }
                else {
                    unreachable!()
                }
            }
        }
    }
}
