// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{create_exception, prelude::*, PyIterProtocol};
use std::{
    borrow::Cow,
    mem::take,
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    api::{self, Build, CompatRule, OptionMap, Request},
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

create_exception!(errors, SolverFailedError, SolverError);

#[pyclass]
pub struct Solver {
    repos: Vec<Arc<Mutex<storage::RepositoryHandle>>>,
    initial_state_builders: Vec<Change>,
    validators: Cow<'static, [Validators]>,
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

                compat = self.validate(&node.state, &spec)?;
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
                            match Decision::build_package(spec.clone(), &repo, &build_env) {
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
                    Decision::resolve_package(&spec, repo)
                };
                decision.add_notes(notes.iter());
                return Ok(Some(decision));
            }
        }

        Err(errors::Error::OutOfOptions(errors::OutOfOptions { request, notes }).into())
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
        Solver {
            repos: Vec::default(),
            initial_state_builders: Vec::default(),
            validators: Cow::from(validation::default_validators()),
        }
    }

    /// Add a repository where the solver can get packages.
    pub fn add_repository(&mut self, repo: storage::python::Repository) {
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
        self.last_graph.read().unwrap().clone()
    }

    pub fn reset(&mut self) {
        self.repos.clear();
        self.initial_state_builders.clear();
        self.validators = Cow::from(validation::default_validators());
    }

    pub fn run(self) -> SolverRuntime {
        SolverRuntime::new(self)
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
        for step in runtime {
            step?;
        }
        Ok(runtime.solution)
    }

    /// Adds requests for all build requirements and solves
    pub fn solve_build_environment(&mut self, spec: &api::Spec) -> Result<Solution> {
        let state = self.get_initial_state();

        let build_options = spec.resolve_all_options(&state.get_option_map());
        for option in &spec.build.options {
            if let api::Opt::Pkg(option) = option {
                let given = build_options.get(&option.pkg);
                let request = option.to_request(given.cloned())?;
                self.add_request(request)
            }
        }

        Ok(self.solve()?)
    }

    pub fn update_options(&mut self, options: OptionMap) {
        self.initial_state_builders
            .push(Change::SetOptions(graph::SetOptions::new(options)))
    }

    fn validate(&self, node: &State, spec: &api::Spec) -> Result<api::Compatibility> {
        for validator in self.validators.as_ref() {
            let compat = validator.validate(node, spec)?;
            if !&compat {
                return Ok(compat);
            }
        }
        Ok(api::Compatibility::Compatible)
    }
}

#[pyclass]
pub struct SolverRuntime {
    solver: Solver,
    graph: Arc<RwLock<Graph>>,
}

impl SolverRuntime {
    pub fn new(solver: Solver) -> Self {
        todo!()
    }
}

impl Iterator for SolverRuntime {
    type Item = Result<(Node, Decision)>;

    fn next(&mut self) -> Option<Self::Item> {
        let solve_graph = Arc::new(RwLock::new(Graph::new()));
        self.last_graph = solve_graph.clone();
        self.steps = Vec::default();

        let mut history = Vec::<Arc<RwLock<Node>>>::new();
        let mut current_node: Option<Arc<RwLock<Node>>> = None;
        let mut decision = Some(Decision::new(self.initial_state_builders.clone()));

        while decision.is_some()
            && (current_node.is_none()
                || !current_node
                    .as_ref()
                    .map(|n| Arc::ptr_eq(&n.read().unwrap().state, &DEAD_STATE))
                    .unwrap_or_default())
        {
            self.steps.push((
                // A clone of Some(current_node) or the root node
                current_node
                    .as_ref()
                    .map(|n| n.read().unwrap().clone())
                    .unwrap_or_else(|| solve_graph.read().unwrap().root.read().unwrap().clone()),
                decision.as_ref().expect("decision is some").clone(),
            ));

            current_node = Some({
                let mut sg = solve_graph.write().unwrap();
                let root_id = sg.root.read().unwrap().id();
                match sg.add_branch(
                    current_node
                        .as_ref()
                        .map(|n| n.read().unwrap().id())
                        .unwrap_or(root_id),
                    decision.unwrap(),
                ) {
                    Ok(cn) => cn,
                    Err(GraphError::RecursionError(msg)) => {
                        match history.pop() {
                            Some(n) => {
                                let n_lock = n.read().unwrap();
                                decision = Some(
                                    Change::StepBack(StepBack::new(
                                        &msg.to_string(),
                                        &n_lock.state,
                                    ))
                                    .as_decision(),
                                )
                            }
                            None => {
                                decision = Some(
                                    Change::StepBack(StepBack::new(&msg.to_string(), &DEAD_STATE))
                                        .as_decision(),
                                )
                            }
                        }
                        continue;
                    }
                }
            });
            let current_node = current_node
                .as_ref()
                .expect("current_node always `is_some` here");
            let mut current_node_lock = current_node.write().unwrap();
            decision = match self.step_state(&mut current_node_lock) {
                Ok(decision) => decision,
                Err(crate::Error::Solve(errors::Error::OutOfOptions(ref err))) => {
                    match history.pop() {
                        Some(n) => {
                            let n_lock = n.read().unwrap();
                            decision = Some(
                                Change::StepBack(StepBack::new(
                                    &format!("could not satisfy '{}'", err.request.pkg),
                                    &n_lock.state,
                                ))
                                .as_decision(),
                            )
                        }
                        None => {
                            decision = Some(
                                Change::StepBack(StepBack::new(
                                    &format!("could not satisfy '{}'", err.request.pkg),
                                    &DEAD_STATE,
                                ))
                                .as_decision(),
                            )
                        }
                    }
                    if let Some(d) = decision.as_mut() {
                        d.add_notes(err.notes.iter())
                    }
                    continue;
                }
                Err(err) => return Err(err.into()),
            };
            history.push(current_node.clone());
        }

        let current_node = current_node.expect("current_node always `is_some` here");
        let current_node_lock = current_node.read().unwrap();

        let is_dead = current_node_lock.state.id()
            == solve_graph.read().unwrap().root.read().unwrap().state.id()
            || Arc::ptr_eq(&current_node_lock.state, &DEAD_STATE);
        let is_empty = self.get_initial_state().get_pkg_requests().is_empty();
        if is_dead && !is_empty {
            Err(SolverFailedError::new_err(
                (*solve_graph).read().unwrap().clone(),
            ))
        } else {
            Ok(current_node_lock.state.as_solution()?)
        }
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
