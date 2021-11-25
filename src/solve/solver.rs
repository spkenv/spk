// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{create_exception, exceptions, prelude::*, PyIterProtocol};
use std::{
    borrow::Cow,
    mem::take,
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    api::{self, Build, CompatRule, Component, OptionMap, Request},
    solve::graph::{GraphError, StepBack},
    storage, Error, Result,
};

use super::{
    errors::{self, SolverError},
    graph::{
        self, Change, Decision, Graph, Node, NoteEnum, RequestPackage, RequestVar, SkipPackageNote,
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

create_exception!(errors, SolverFailedError, SolverError);

#[pyclass]
#[derive(Clone)]
pub struct Solver {
    repos: Vec<Arc<Mutex<storage::RepositoryHandle>>>,
    initial_state_builders: Vec<Change>,
    validators: Cow<'static, [Validators]>,
}

impl Default for Solver {
    fn default() -> Self {
        Self {
            repos: Vec::default(),
            initial_state_builders: Vec::default(),
            validators: Cow::from(validation::default_validators()),
        }
    }
}

// Methods not exposed to Python
impl Solver {
    /// Add a request to this solver.
    pub fn add_request(&mut self, request: api::Request) {
        let request = match request {
            Request::Pkg(request) => Change::RequestPackage(RequestPackage::new(request)),
            Request::Var(request) => Change::RequestVar(RequestVar::new(request)),
        };
        self.initial_state_builders.push(request);
    }

    /// Add a repository where the solver can get packages.
    pub fn add_repository(&mut self, repo: Arc<Mutex<storage::RepositoryHandle>>) {
        self.repos.push(repo);
    }

    fn get_iterator(
        &self,
        node: &mut Node,
        package_name: &str,
    ) -> Arc<Mutex<Box<dyn PackageIterator>>> {
        if let Some(iterator) = node.get_iterator(package_name) {
            return iterator;
        }
        let iterator = self.make_iterator(package_name);
        node.set_iterator(package_name, &iterator);
        iterator
    }

    fn make_iterator(&self, package_name: &str) -> Arc<Mutex<Box<dyn PackageIterator>>> {
        assert!(!self.repos.is_empty());
        Arc::new(Mutex::new(Box::new(RepositoryPackageIterator::new(
            package_name.to_owned(),
            self.repos.clone(),
        ))))
    }

    fn resolve_new_build(&self, spec: &api::Spec, state: &State) -> Result<Solution> {
        let mut opts = state.get_option_map();
        for pkg_request in state.get_pkg_requests() {
            if !opts.contains_key(pkg_request.pkg.name()) {
                opts.insert(
                    pkg_request.pkg.name().to_owned(),
                    pkg_request.pkg.version.to_string(),
                );
            }
        }
        for var_request in state.get_var_requests() {
            if !opts.contains_key(&var_request.var) {
                opts.insert(var_request.var.clone(), var_request.value.clone());
            }
        }

        let mut solver = Solver::new();
        solver.repos = self.repos.clone();
        solver.update_options(opts);
        solver.solve_build_environment(spec)
    }

    fn step_state(&self, node: &mut Node) -> Result<Option<Decision>> {
        let mut notes = Vec::<NoteEnum>::new();
        let request = if let Some(request) = node.state.get_next_request()? {
            request
        } else {
            return Ok(None);
        };

        let iterator = self.get_iterator(node, request.pkg.name());
        let mut iterator_lock = iterator.lock().unwrap();
        while let Some((pkg, builds)) = iterator_lock.next()? {
            let mut compat = request.is_version_applicable(&pkg.version);
            if !&compat {
                iterator_lock.set_builds(
                    &pkg.version,
                    Arc::new(Mutex::new(EmptyBuildIterator::new())),
                );
                notes.push(NoteEnum::SkipPackageNote(SkipPackageNote::new(
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
                let build_from_source = spec.pkg.build == Some(Build::Source)
                    && request.pkg.build != Some(Build::Source);
                if build_from_source {
                    if let PackageSource::Spec(spec) = repo {
                        notes.push(NoteEnum::SkipPackageNote(
                            SkipPackageNote::new_from_message(
                                spec.pkg.clone(),
                                "cannot build embedded source package",
                            ),
                        ));
                        continue;
                    }

                    match repo.read_spec(&spec.pkg.with_build(None)) {
                        Ok(s) => spec = Arc::new(s),
                        Err(Error::PackageNotFoundError(pkg)) => {
                            notes.push(NoteEnum::SkipPackageNote(
                                SkipPackageNote::new_from_message(
                                    pkg,
                                    "cannot build from source, version spec not available",
                                ),
                            ));
                            continue;
                        }
                        Err(err) => return Err(err),
                    }
                }

                compat = self.validate(&node.state, &spec, &repo)?;
                if !&compat {
                    notes.push(NoteEnum::SkipPackageNote(SkipPackageNote::new(
                        spec.pkg.clone(),
                        compat,
                    )));
                    continue;
                }

                let mut decision = if build_from_source {
                    match self.resolve_new_build(&spec, &node.state) {
                        Ok(build_env) => {
                            match Decision::builder(spec.clone())
                                .with_components(request.pkg.components.clone())
                                .build_package(&build_env)
                            {
                                Ok(decision) => decision,
                                Err(err) => {
                                    notes.push(NoteEnum::SkipPackageNote(
                                        SkipPackageNote::new_from_message(
                                            spec.pkg.clone(),
                                            &format!("cannot build package: {:?}", err),
                                        ),
                                    ));
                                    continue;
                                }
                            }
                        }

                        // FIXME: This should only match `SolverError`
                        Err(err) => {
                            notes.push(NoteEnum::SkipPackageNote(
                                SkipPackageNote::new_from_message(
                                    spec.pkg.clone(),
                                    &format!("cannot resolve build env: {:?}", err),
                                ),
                            ));
                            continue;
                        }
                    }
                } else {
                    Decision::builder(spec)
                        .with_components(request.pkg.components.clone())
                        .resolve_package(repo)
                };
                decision.add_notes(notes.iter());
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
            let compat = validator.validate(node, spec, &source)?;
            if !&compat {
                return Ok(compat);
            }
        }
        Ok(api::Compatibility::Compatible)
    }
}

#[derive(FromPyObject)]
pub enum RequestEnum {
    Ident(api::Ident),
    Request(api::Request),
    String(String),
}

#[pymethods]
impl Solver {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    /// Add a repository where the solver can get packages.
    #[pyo3(name = "add_repository")]
    pub fn py_add_repository(&mut self, repo: storage::python::Repository) {
        self.repos.push(repo.handle);
    }

    /// Add a request to this solver.
    #[pyo3(name = "add_request")]
    pub fn py_add_request(&mut self, request: RequestEnum) -> PyResult<()> {
        let mut request = request;
        let request = loop {
            match request {
                RequestEnum::Ident(r) => {
                    request = RequestEnum::String(r.to_string());
                    continue;
                }
                RequestEnum::String(request) => {
                    let mut request = serde_yaml::from_str::<api::PkgRequest>(&format!(
                        "{{\"pkg\": {}}}",
                        request
                    ))
                    .map_err(|err| {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err.to_string())
                    })?;
                    request.required_compat = Some(CompatRule::API);
                    if request.pkg.components.is_empty() {
                        request.pkg.components.insert(Component::Run);
                    }
                    break api::Request::Pkg(request);
                }
                RequestEnum::Request(request) => break request,
            }
        };
        self.add_request(request);
        Ok(())
    }

    pub fn get_initial_state(&self) -> State {
        let mut state = State::default();
        for change in self.initial_state_builders.iter() {
            state = change.apply(&state)
        }
        state
    }

    pub fn get_last_solve_graph(&self) -> Graph {
        //self.last_graph.read().unwrap().clone()
        todo!()
    }

    pub fn reset(&mut self) {
        self.repos.clear();
        self.initial_state_builders.clear();
        self.validators = Cow::from(validation::default_validators());
    }

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

    pub fn solve(&mut self) -> PyResult<Solution> {
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
                let given = build_options.get(&option.pkg);
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
        Ok(self.solve()?)
    }

    pub fn update_options(&mut self, options: OptionMap) {
        self.initial_state_builders
            .push(Change::SetOptions(graph::SetOptions::new(options)))
    }
}

#[pyclass]
pub struct SolverRuntime {
    solver: Solver,
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

    pub fn graph(&self) -> Arc<RwLock<Graph>> {
        self.graph.clone()
    }

    pub fn iter(&mut self) -> SolverRuntimeIter<'_> {
        SolverRuntimeIter(self)
    }
}

#[pymethods]
impl SolverRuntime {
    #[pyo3(name = "graph")]
    pub fn pygraph(&self) -> Graph {
        self.graph.read().unwrap().clone()
    }

    /// Returns the completed solution for this runtime.
    ///
    /// If needed, this function will iterate any remaining
    /// steps for the current state.
    pub fn solution(&mut self) -> PyResult<Solution> {
        for item in self.iter() {
            item?;
        }
        self.current_solution()
    }

    /// Return the current solution for this runtime.
    ///
    /// If the runtime has not yet completed, this solution
    /// may be incomplete or empty.
    pub fn current_solution(&self) -> PyResult<Solution> {
        let current_node = self.current_node.as_ref().ok_or_else(|| {
            exceptions::PyRuntimeError::new_err("Solver runtime as not been consumed")
        })?;
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
            Err(SolverFailedError::new_err(
                (*self.graph).read().unwrap().clone(),
            ))
        } else {
            current_node_lock.state.as_solution()
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
                Err(GraphError::RecursionError(msg)) => {
                    match self.history.pop() {
                        Some(n) => {
                            let n_lock = n.read().unwrap();
                            self.decision = Some(
                                Change::StepBack(StepBack::new(&msg.to_string(), &n_lock.state))
                                    .as_decision(),
                            )
                        }
                        None => {
                            self.decision = Some(
                                Change::StepBack(StepBack::new(&msg.to_string(), &DEAD_STATE))
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
                match self.history.pop() {
                    Some(n) => {
                        let n_lock = n.read().unwrap();
                        self.decision = Some(
                            Change::StepBack(StepBack::new(
                                &format!("could not satisfy '{}'", err.request.pkg),
                                &n_lock.state,
                            ))
                            .as_decision(),
                        )
                    }
                    None => {
                        self.decision = Some(
                            Change::StepBack(StepBack::new(
                                &format!("could not satisfy '{}'", err.request.pkg),
                                &DEAD_STATE,
                            ))
                            .as_decision(),
                        )
                    }
                }
                if let Some(d) = self.decision.as_mut() {
                    d.add_notes(err.notes.iter())
                }
                return Some(Ok(to_yield));
            }
            Err(err) => return Some(Err(err)),
        };
        self.history.push(current_node.clone());
        Some(Ok(to_yield))
    }
}

#[pyproto]
impl PyIterProtocol for SolverRuntime {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<Self>) -> Result<Option<(Node, Decision)>> {
        match slf.next() {
            Some(Ok(i)) => Ok(Some(i)),
            Some(Err(err)) => Err(err),
            None => Ok(None),
        }
    }
}

pub struct SolverRuntimeIter<'a>(&'a mut SolverRuntime);

impl<'a> Iterator for SolverRuntimeIter<'a> {
    type Item = Result<(Node, Decision)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}
