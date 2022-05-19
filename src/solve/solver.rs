// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    borrow::Cow,
    collections::HashMap,
    mem::take,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
};

use crate::{
    api::{self, Build, Component, OptionMap, PkgName, Request},
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
        EmptyBuildIterator, PackageIterator, RepositoryPackageIterator, SortedBuildIterator,
    },
    solution::{PackageSource, Solution},
    validation::{self, BinaryOnlyValidator, ValidatorT, Validators},
};

#[cfg(test)]
#[path = "./solver_test.rs"]
mod solver_test;

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
        }
    }
}

impl Solver {
    /// Add a request to this solver.
    pub fn add_request(&mut self, request: api::Request) {
        let request = match request {
            Request::Pkg(mut request) => {
                if request.pkg.components.is_empty() {
                    request.pkg.components.insert(api::Component::Run);
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
        let else_closure = || Arc::new(State::default());
        for change in self.initial_state_builders.iter() {
            state = Some(change.apply(&state.unwrap_or_else(else_closure)))
        }
        state.unwrap_or_else(else_closure)
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

    fn get_iterator(
        &self,
        node: &mut Node,
        package_name: &PkgName,
    ) -> Arc<Mutex<Box<dyn PackageIterator>>> {
        if let Some(iterator) = node.get_iterator(package_name) {
            return iterator;
        }
        let iterator = self.make_iterator(package_name.clone());
        node.set_iterator(package_name.clone(), &iterator);
        iterator
    }

    fn make_iterator(&self, package_name: api::PkgName) -> Arc<Mutex<Box<dyn PackageIterator>>> {
        debug_assert!(!self.repos.is_empty());
        Arc::new(Mutex::new(Box::new(RepositoryPackageIterator::new(
            package_name,
            self.repos.clone(),
        ))))
    }

    fn resolve_new_build(&self, spec: &api::Spec, state: &State) -> Result<Solution> {
        let mut opts = state.get_option_map();
        for pkg_request in state.get_pkg_requests() {
            if !opts.contains_key(pkg_request.pkg.name.as_str()) {
                opts.insert(
                    pkg_request.pkg.name.to_string(),
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
        solver.update_options(opts);
        solver.solve_build_environment(spec)
    }

    fn step_state(&mut self, node: &mut Node) -> Result<Option<Decision>> {
        let mut notes = Vec::<Note>::new();
        let request = if let Some(request) = node.state.get_next_request()? {
            request
        } else {
            return Ok(None);
        };

        // This is a step forward in the solve
        self.number_of_steps += 1;

        let iterator = self.get_iterator(node, &request.pkg.name);
        let mut iterator_lock = iterator.lock().unwrap();
        while let Some((pkg, builds)) = iterator_lock.next()? {
            let mut compat = request.is_version_applicable(&pkg.version);
            if !&compat {
                // Count this version and its builds as incompatible
                self.number_incompat_versions += 1;
                self.number_incompat_builds += builds.lock().unwrap().len();

                // Skip this version and move on the to next one
                iterator_lock.set_builds(
                    &pkg.version,
                    Arc::new(Mutex::new(EmptyBuildIterator::new())),
                );
                notes.push(Note::SkipPackageNote(SkipPackageNote::new(
                    pkg.clone(),
                    compat,
                )));
                continue;
            }

            let builds = if !builds.lock().unwrap().is_sorted_build_iterator() {
                let builds = Arc::new(Mutex::new(SortedBuildIterator::new(
                    node.state.get_option_map(),
                    builds.clone(),
                )?));
                iterator_lock.set_builds(&pkg.version, builds.clone());
                builds
            } else {
                builds
            };

            while let Some((mut spec, repo)) = builds.lock().unwrap().next()? {
                // Now all this build to the total considered during
                // this overall step
                self.number_total_builds += 1;

                let build_from_source = spec.pkg.build == Some(Build::Source)
                    && request.pkg.build != Some(Build::Source);
                if build_from_source {
                    if let PackageSource::Spec(spec) = repo {
                        notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                            spec.pkg.clone(),
                            "cannot build embedded source package",
                        )));
                        self.number_builds_skipped += 1;
                        continue;
                    }

                    match repo.read_spec(&spec.pkg.with_build(None)) {
                        Ok(s) => spec = Arc::new(s),
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

                compat = self.validate(&node.state, &spec, &repo)?;
                if !&compat {
                    notes.push(Note::SkipPackageNote(SkipPackageNote::new(
                        spec.pkg.clone(),
                        compat,
                    )));
                    self.number_builds_skipped += 1;
                    continue;
                }

                let mut decision = if build_from_source {
                    match self.resolve_new_build(&spec, &node.state) {
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

    pub fn solve(&mut self) -> Result<Solution> {
        let mut runtime = self.run();
        for step in runtime.iter() {
            step?;
        }
        runtime.current_solution()
    }

    /// Adds requests for all build requirements
    pub fn configure_for_build_environment(&mut self, spec: &api::Spec) -> Result<()> {
        let state = self.get_initial_state();

        let build_options = spec.resolve_all_options(&state.get_option_map());
        for option in &spec.build.options {
            if let api::Opt::Pkg(option) = option {
                let given = build_options.get(option.pkg.as_str());
                let mut request = option.to_request(given.cloned())?;
                // if no components were explicitly requested in a build option,
                // then we inject the default for this context
                if request.pkg.components.is_empty() {
                    request.pkg.components.insert(Component::Build);
                }
                self.add_request(request.into())
            }
        }

        Ok(())
    }

    /// Adds requests for all build requirements and solves
    pub fn solve_build_environment(&mut self, spec: &api::Spec) -> Result<Solution> {
        self.configure_for_build_environment(spec)?;
        self.solve()
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

#[must_use = "The solver runtime does nothing unless iterated to completion"]
pub struct SolverRuntime {
    pub solver: Solver,
    graph: Arc<RwLock<Graph>>,
    history: Vec<Arc<RwLock<Node>>>,
    current_node: Option<Arc<RwLock<Node>>>,
    decision: Option<Decision>,
}

impl SolverRuntime {
    pub fn new(solver: Solver) -> Self {
        let initial_decision = Decision::new(solver.initial_state_builders.clone());
        Self {
            solver,
            graph: Arc::new(RwLock::new(Graph::new())),
            history: Vec::new(),
            current_node: None,
            decision: Some(initial_decision),
        }
    }

    /// A reference to the solve graph being built by this runtime
    pub fn graph(&self) -> Arc<RwLock<Graph>> {
        self.graph.clone()
    }

    /// Iterate through each step of this runtime, trying to converge on a solution
    pub fn iter(&mut self) -> SolverRuntimeIter<'_> {
        SolverRuntimeIter(self)
    }

    /// Returns the completed solution for this runtime.
    ///
    /// If needed, this function will iterate any remaining
    /// steps for the current state.
    pub fn solution(&mut self) -> Result<Solution> {
        let _guard = crate::HANDLE.enter();
        for item in self.iter() {
            item?;
        }
        self.current_solution()
    }

    /// Return the current solution for this runtime.
    ///
    /// If the runtime has not yet completed, this solution
    /// may be incomplete or empty.
    pub fn current_solution(&self) -> Result<Solution> {
        let current_node = self
            .current_node
            .as_ref()
            .ok_or_else(|| Error::String("Solver runtime has not been consumed".into()))?;
        let current_node_lock = current_node.read().unwrap();

        let is_dead = current_node_lock.state.id()
            == self.graph.read().unwrap().root.read().unwrap().state.id()
            || Arc::ptr_eq(&current_node_lock.state, &DEAD_STATE);
        let is_empty = self
            .solver
            .get_initial_state()
            .get_pkg_requests()
            .is_empty();
        if is_dead && !is_empty {
            Err(super::Error::FailedToResolve((*self.graph).read().unwrap().clone()).into())
        } else {
            current_node_lock
                .state
                .as_solution()
                // TODO: make a proper conversion for this
                .map_err(|e| crate::Error::String(e.to_string()))
        }
    }
}

impl Iterator for SolverRuntime {
    type Item = Result<(Node, Decision)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.decision.is_none()
            || (self.current_node.is_some()
                && self
                    .current_node
                    .as_ref()
                    .map(|n| Arc::ptr_eq(&n.read().unwrap().state, &DEAD_STATE))
                    .unwrap_or_default())
        {
            return None;
        }

        let to_yield = (
            // A clone of Some(current_node) or the root node
            self.current_node
                .as_ref()
                .map(|n| n.read().unwrap().clone())
                .unwrap_or_else(|| self.graph.read().unwrap().root.read().unwrap().clone()),
            self.decision.as_ref().expect("decision is some").clone(),
        );

        self.current_node = Some({
            let mut sg = self.graph.write().unwrap();
            let root_id = sg.root.read().unwrap().id();
            match sg.add_branch(
                self.current_node
                    .as_ref()
                    .map(|n| n.read().unwrap().id())
                    .unwrap_or(root_id),
                self.decision.take().unwrap(),
            ) {
                Ok(cn) => cn,
                Err(err) => {
                    match self.history.pop() {
                        Some(n) => {
                            let n_lock = n.read().unwrap();
                            self.decision = Some(
                                Change::StepBack(StepBack::new(
                                    err.to_string(),
                                    &n_lock.state,
                                    Arc::clone(&self.solver.number_of_steps_back),
                                ))
                                .as_decision(),
                            )
                        }
                        None => {
                            self.decision = Some(
                                Change::StepBack(StepBack::new(
                                    err.to_string(),
                                    &DEAD_STATE,
                                    Arc::clone(&self.solver.number_of_steps_back),
                                ))
                                .as_decision(),
                            )
                        }
                    }
                    return Some(Ok(to_yield));
                }
            }
        });
        let current_node = self
            .current_node
            .as_ref()
            .expect("current_node always `is_some` here");
        let mut current_node_lock = current_node.write().unwrap();
        self.decision = match self.solver.step_state(&mut current_node_lock) {
            Ok(decision) => decision,
            Err(crate::Error::Solve(errors::Error::OutOfOptions(ref err))) => {
                let cause = format!("could not satisfy '{}'", err.request.pkg);

                match self.history.pop() {
                    Some(n) => {
                        let n_lock = n.read().unwrap();
                        self.decision = Some(
                            Change::StepBack(StepBack::new(
                                &cause,
                                &n_lock.state,
                                Arc::clone(&self.solver.number_of_steps_back),
                            ))
                            .as_decision(),
                        )
                    }
                    None => {
                        self.decision = Some(
                            Change::StepBack(StepBack::new(
                                &cause,
                                &DEAD_STATE,
                                Arc::clone(&self.solver.number_of_steps_back),
                            ))
                            .as_decision(),
                        )
                    }
                }

                self.solver.increment_error_count(cause);

                if let Some(d) = self.decision.as_mut() {
                    d.add_notes(err.notes.iter().cloned())
                }
                return Some(Ok(to_yield));
            }
            Err(err) => return Some(Err(err)),
        };
        self.history.push(current_node.clone());
        Some(Ok(to_yield))
    }
}

pub struct SolverRuntimeIter<'a>(&'a mut SolverRuntime);

impl<'a> Iterator for SolverRuntimeIter<'a> {
    type Item = Result<(Node, Decision)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}
