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

use crate::option_map::OptionMap;
use async_stream::stream;
use futures::{Stream, TryStreamExt};
use priority_queue::priority_queue::PriorityQueue;
use spk_foundation::ident_build::Build;
use spk_foundation::ident_component::Component;
use spk_foundation::spec_ops::{PackageOps, RecipeOps};
use spk_ident::{Ident, PkgRequest, Request, RequestedBy, VarRequest};
use spk_name::{PkgName, PkgNameBuf};
use spk_solver_graph::{
    Change, Decision, Graph, Node, Note, RequestPackage, RequestVar, SetOptions, SkipPackageNote,
    State, StepBack, DEAD_STATE,
};
use spk_solver_package_iterator::{
    EmptyBuildIterator, PackageIterator, RepositoryPackageIterator, SortedBuildIterator,
};
use spk_solver_solution::{PackageSource, Solution};
use spk_solver_validation::{default_validators, BinaryOnlyValidator, ValidatorT, Validators};
use spk_spec::{Deprecate, Package, Recipe, Spec, SpecRecipe};
use spk_storage::RepositoryHandle;
use spk_version::Compatibility;

use crate::{Error, Result};

use super::error;

// Public to allow other tests to use its macros
#[cfg(test)]
#[path = "./solver_test.rs"]
mod solver_test;

#[derive(Clone)]
pub struct Solver {
    repos: Vec<Arc<RepositoryHandle>>,
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
            validators: Cow::from(default_validators()),
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
    pub fn add_request(&mut self, request: Request) {
        let request = match request {
            Request::Pkg(mut request) => {
                if request.pkg.components.is_empty() {
                    request.pkg.components.insert(Component::default_for_run());
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
        R: Into<Arc<RepositoryHandle>>,
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
        package_name: PkgNameBuf,
    ) -> Arc<tokio::sync::Mutex<Box<dyn PackageIterator + Send>>> {
        debug_assert!(!self.repos.is_empty());
        Arc::new(tokio::sync::Mutex::new(Box::new(
            RepositoryPackageIterator::new(package_name, self.repos.clone()),
        )))
    }

    /// Resolve the build environment, and generate a build for
    /// the given recipe and state.
    ///
    /// The returned spec describes the package that should be built
    /// in order to satisfy the set of requests in the provided state.
    /// The build environment for the package is resolved in order to
    /// validate that a build is possible and to generate the resulting
    /// spec.
    #[async_recursion::async_recursion]
    async fn resolve_new_build(&self, recipe: &SpecRecipe, state: &State) -> Result<Arc<Spec>> {
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
        let solution = solver.solve_build_environment(recipe).await?;
        recipe
            .generate_binary_build(&opts, &solution)
            .map_err(|err| err.into())
            .map(Arc::new)
    }

    async fn step_state(&mut self, node: &mut Arc<Node>) -> Result<Option<Decision>> {
        let mut notes = Vec::<Note>::new();
        let request = if let Some(request) = node.state.get_next_request()? {
            request
        } else {
            return Ok(None);
        };

        // This is a step forward in the solve
        self.number_of_steps += 1;

        let iterator = self.get_iterator(node, &request.pkg.name).await;
        let mut iterator_lock = iterator.lock().await;
        loop {
            let (pkg, builds) = match iterator_lock.next().await {
                Ok(Some((pkg, builds))) => (pkg, builds),
                Ok(None) => break,
                Err(spk_solver_package_iterator::Error::SpkValidatorsError(
                    spk_validators::Error::PackageNotFoundError(_),
                )) => {
                    // Intercept this error in this situation to
                    // capture the request for the package that turned
                    // out to be missing.
                    return Err(spk_solver_graph::Error::PackageNotFoundDuringSolve(
                        request.clone(),
                    )
                    .into());
                }
                Err(e) => return Err(e.into()),
            };

            let mut compat = request.is_version_applicable(&pkg.version);
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

            while let Some(hm) = builds.lock().await.next().await? {
                // Now add this build to the total considered during
                // this overall step

                self.number_total_builds += 1;

                // Try all the hash map values to check all repos.
                for (spec, source) in hm.values() {
                    let spec = Arc::clone(spec);
                    let build_from_source =
                        spec.ident().is_source() && request.pkg.build != Some(Build::Source);

                    let mut decision = if !build_from_source {
                        compat = self.validate_package(&node.state, &spec, source)?;
                        if !&compat {
                            notes.push(Note::SkipPackageNote(SkipPackageNote::new(
                                spec.ident().clone(),
                                compat.clone(),
                            )));
                            self.number_builds_skipped += 1;
                            continue;
                        }
                        Decision::builder(&node.state)
                            .with_components(&request.pkg.components)
                            .resolve_package(&spec, source.clone())
                    } else {
                        if let PackageSource::Embedded = source {
                            notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                                spec.ident().clone(),
                                &compat,
                            )));
                            self.number_builds_skipped += 1;
                            continue;
                        }
                        let recipe = match source.read_recipe(spec.ident()).await {
                            Ok(r) if r.is_deprecated() => {
                                notes.push(Note::SkipPackageNote(
                                    SkipPackageNote::new_from_message(
                                        pkg.clone(),
                                        "cannot build from source, version is deprecated",
                                    ),
                                ));
                                continue;
                            }
                            Ok(r) => r,
                            Err(spk_solver_solution::Error::SpkStorageError(
                                spk_storage::Error::SpkValidatorsError(
                                    spk_validators::Error::PackageNotFoundError(pkg),
                                ),
                            )) => {
                                notes.push(Note::SkipPackageNote(
                                    SkipPackageNote::new_from_message(
                                        pkg,
                                        "cannot build from source, recipe not available",
                                    ),
                                ));
                                continue;
                            }
                            Err(err) => return Err(err.into()),
                        };
                        compat = self.validate_recipe(&node.state, &recipe)?;
                        if !&compat {
                            notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                                spec.ident().clone(),
                                format!("recipe is not valid: {compat}"),
                            )));
                            self.number_builds_skipped += 1;
                            continue;
                        }

                        let new_spec = match self.resolve_new_build(&*recipe, &node.state).await {
                            Err(err) => {
                                notes.push(Note::SkipPackageNote(
                                    SkipPackageNote::new_from_message(
                                        spec.ident().clone(),
                                        format!("cannot resolve build env: {err}"),
                                    ),
                                ));
                                self.number_builds_skipped += 1;
                                continue;
                            }
                            res => res?,
                        };
                        let new_source = PackageSource::BuildFromSource {
                            recipe: Arc::clone(&recipe),
                        };

                        compat = self.validate_package(&node.state, &new_spec, &new_source)?;
                        if !&compat {
                            notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                                spec.ident().clone(),
                                format!("built package would still be invalid: {compat}"),
                            )));
                            self.number_builds_skipped += 1;
                            continue;
                        }

                        match Decision::builder(&node.state)
                            .with_components(&request.pkg.components)
                            .build_package(&recipe, &new_spec)
                        {
                            Ok(decision) => decision,
                            Err(err) => {
                                notes.push(Note::SkipPackageNote(
                                    SkipPackageNote::new_from_message(
                                        spec.ident().clone(),
                                        format!("cannot build package: {err}"),
                                    ),
                                ));
                                self.number_builds_skipped += 1;
                                continue;
                            }
                        }
                    };
                    decision.add_notes(notes.iter().cloned());
                    return Ok(Some(decision));
                }
            }
        }

        Err(error::Error::OutOfOptions(error::OutOfOptions {
            request,
            notes,
        }))
    }

    fn validate_recipe<R: Recipe>(&self, node: &State, recipe: &R) -> Result<Compatibility> {
        for validator in self.validators.as_ref() {
            let compat = validator.validate_recipe(node, recipe)?;
            if !&compat {
                return Ok(compat);
            }
        }
        Ok(Compatibility::Compatible)
    }

    fn validate_package<P: Package>(
        &self,
        node: &State,
        spec: &P,
        source: &PackageSource,
    ) -> Result<Compatibility>
    where
        P: Package<Ident = Ident>,
        P: PackageOps<VarRequest = VarRequest>,
        P: RecipeOps<PkgRequest = PkgRequest>,
    {
        for validator in self.validators.as_ref() {
            let compat = validator.validate_package(node, spec, source)?;
            if !&compat {
                return Ok(compat);
            }
        }
        Ok(Compatibility::Compatible)
    }

    /// Put this solver back into its default state
    pub fn reset(&mut self) {
        self.repos.truncate(0);
        self.initial_state_builders.truncate(0);
        self.validators = Cow::from(default_validators());
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
    pub fn configure_for_build_environment<T: Recipe>(&mut self, recipe: &T) -> Result<()> {
        let state = self.get_initial_state();

        let build_options = recipe.resolve_options(state.get_option_map())?;
        for req in recipe.get_build_requirements(&build_options)? {
            self.add_request(req)
        }

        Ok(())
    }

    /// Adds requests for all build requirements and solves
    pub async fn solve_build_environment(&mut self, recipe: &SpecRecipe) -> Result<Solution> {
        self.configure_for_build_environment(recipe)?;
        self.solve().await
    }

    pub fn update_options(&mut self, options: OptionMap) {
        self.initial_state_builders
            .push(Change::SetOptions(SetOptions::new(options)))
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

// This is needed so `PriorityQueue` doesn't need to hash the node itself.
struct NodeWrapper {
    pub(crate) node: Arc<tokio::sync::RwLock<Arc<Node>>>,
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

type SolverHistory = PriorityQueue<NodeWrapper, std::cmp::Reverse<u64>>;

#[must_use = "The solver runtime does nothing unless iterated to completion"]
pub struct SolverRuntime {
    pub solver: Solver,
    graph: Arc<tokio::sync::RwLock<Graph>>,
    history: SolverHistory,
    current_node: Option<Arc<tokio::sync::RwLock<Arc<Node>>>>,
    decision: Option<Arc<Decision>>,
}

impl SolverRuntime {
    pub fn new(solver: Solver) -> Self {
        let initial_decision = Decision::new(solver.initial_state_builders.clone());
        Self {
            solver,
            graph: Arc::new(tokio::sync::RwLock::new(Graph::new())),
            history: SolverHistory::default(),
            current_node: None,
            decision: Some(Arc::new(initial_decision)),
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
        let current_node = self
            .current_node
            .as_ref()
            .ok_or_else(|| Error::String("Solver runtime has not been consumed".into()))?;
        let current_node_lock = current_node.read().await;

        let is_dead = current_node_lock.state.id()
            == self.graph.read().await.root.read().await.state.id()
            || Arc::ptr_eq(&current_node_lock.state, &DEAD_STATE);
        let is_empty = self
            .solver
            .get_initial_state()
            .get_pkg_requests()
            .is_empty();
        if is_dead && !is_empty {
            Err(spk_solver_graph::Error::FailedToResolve((*self.graph).read().await.clone()).into())
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
        decision: &mut Option<Arc<Decision>>,
        solver: &Solver,
        message: &String,
    ) {
        // After encountering a solver error, start trying a new path from the
        // oldest fork. Experimentation shows that this is able to discover
        // a valid solution must faster than going back to the newest fork,
        // for problem cases that get stuck in a bad path.
        match history.pop() {
            Some((n, _)) => {
                let n_lock = n.node.read().await;
                *decision = Some(Arc::new(
                    Change::StepBack(StepBack::new(
                        message,
                        &n_lock.state,
                        Arc::clone(&solver.number_of_steps_back),
                    ))
                    .as_decision(),
                ))
            }
            None => {
                *decision = Some(Arc::new(
                    Change::StepBack(StepBack::new(
                        message,
                        &DEAD_STATE,
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
                if self.decision.is_none()
                    || (self.current_node.is_some()
                        && {
                            if let Some(n) = self.current_node.as_ref() {
                                Arc::ptr_eq(&n.read().await.state, &DEAD_STATE)
                            }
                            else {
                                false
                            }
                        })
                {
                    break 'outer;
                }

                let to_yield = (
                    // A clone of Some(current_node) or the root node
                    {
                        if let Some(n) = self.current_node.as_ref() {
                            n.read().await.clone()
                        }
                        else {
                            self.graph.read().await.root.read().await.clone()
                        }
                    },
                    self.decision.as_ref().expect("decision is some").clone(),
                );

                self.current_node = Some({
                    let mut sg = self.graph.write().await;
                    let root_id = sg.root.read().await.id();
                    match sg.add_branch({
                            if let Some(n) = self.current_node.as_ref() {
                                n.read().await.id()
                            }
                            else {
                                root_id
                            }},
                            self.decision.take().unwrap(),
                        ).await {
                        Ok(cn) => cn,
                        Err(err) => {
                            SolverRuntime::take_a_step_back(
                                &mut self.history,
                                &mut self.decision,
                                &self.solver,
                                &err.to_string(),
                            ).await;
                            yield Ok(to_yield);
                            continue 'outer;
                        }
                    }
                });
                let current_node = self
                    .current_node
                    .as_ref()
                    .expect("current_node always `is_some` here");
                let mut current_node_lock = current_node.write().await;
                let current_level = current_node_lock.state.state_depth;
                self.decision = match self.solver.step_state(&mut current_node_lock).await
                {
                    Ok(decision) => decision.map(Arc::new),
                    Err(crate::Error::OutOfOptions(ref err)) => {
                        // Add to problem package counts based on what made
                        // the request for the blocked package.
                        let requested_by = err.request.get_requesters();
                        for req in &requested_by {
                            if let RequestedBy::PackageBuild(problem_package) = req {
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

                        SolverRuntime::take_a_step_back(
                            &mut self.history,
                            &mut self.decision,
                            &self.solver,
                            &cause,
                        ).await;

                        self.solver.increment_error_count(cause);

                        if let Some(d) = self.decision.as_mut() {
                            Arc::make_mut(d).add_notes(err.notes.iter().cloned())
                        }
                        yield Ok(to_yield);
                        continue 'outer;
                    }
                    Err(Error::SpkSolverGraphError(spk_solver_graph::Error::PackageNotFoundDuringSolve(err_req))) => {
                        let requested_by = err_req.get_requesters();
                        for req in &requested_by {
                            // Can't recover from a command line request for a
                            // missing package.
                            if let RequestedBy::CommandLine = req {
                                yield Err(Error::SpkSolverGraphError(spk_solver_graph::Error::PackageNotFoundDuringSolve(err_req)));
                                continue 'outer;
                            }

                            // Add to problem package counts based on what
                            // made the request for the blocked package.
                            if let RequestedBy::PackageBuild(problem_package) = req {
                                self.solver
                                    .increment_problem_package_count(problem_package.name.to_string())
                            };
                        }

                        let requirers: Vec<String> = requested_by.iter().map(ToString::to_string).collect();
                        let cause = format!("Package '{}' not found during the solve as required by: {}. Please check the spelling of the package's name", err_req.pkg, requirers.join(", "));

                        SolverRuntime::take_a_step_back(
                            &mut self.history,
                            &mut self.decision,
                            &self.solver,
                            &cause,
                        ).await;

                        self.solver.increment_error_count(cause);

                        // This doesn't halt the solve because the missing
                        // package might only have been requested by one
                        // build. It may not be required in the next build the
                        // solver looks at. However, this kind of error
                        // usually occurs either because of command line
                        // request that was mistyped, or because a requirement
                        // was misspelt in a yaml file that was given on a
                        // command line. So the solver is likely to hit a dead
                        // end and halt fairly soon.
                        yield Ok(to_yield);
                        continue 'outer;
                    }
                    Err(err) => {
                        let cause = format!("{}", err);
                        self.solver.increment_error_count(cause);
                        yield Err(err);
                        continue 'outer;
                    }
                };
                self.history.push(
                    NodeWrapper {
                        node: current_node.clone(),
                        // I tried reversing the order of the hash here and it
                        // produced the same result.
                        hash: self.solver.get_number_of_steps() as u64,
                    },
                    std::cmp::Reverse(current_level),
                );
                yield Ok(to_yield)
            }
        }
    }
}
