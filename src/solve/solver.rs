// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{create_exception, prelude::*};
use std::sync::{Arc, RwLock};

use crate::{
    api::{self, Build, CompatRule, OptionMap, Request},
    solve::graph::{ChangeBaseT, GraphError, StepBack},
};

use super::{
    errors::{self, SolverError},
    graph::{
        self, Changes, Decision, Graph, Node, NoteEnum, RequestPackage, RequestVar,
        SkipPackageNote, State, DEAD_STATE,
    },
    package_iterator::{BuildIterator, PackageIterator, SortedBuildIterator},
    solution::{PackageSource, Solution},
    validation::{self, BinaryOnlyValidator, Validators},
};

create_exception!(errors, SolverFailedError, SolverError);

#[pyclass]
pub struct Solver {
    repos: Vec<PyObject>,
    initial_state_builders: Vec<Changes>,
    validators: Vec<Validators>,
    last_graph: Arc<RwLock<Graph>>,
}

// Methods not exposed to Python
impl Solver {
    fn get_iterator(&self, _node: &Node, _package_name: &str) -> PackageIterator {
        todo!()
    }

    fn resolve_new_build(&self, _spec: &api::Spec, _state: &State) -> errors::Result<Solution> {
        todo!()
    }

    fn step_state(&self, node: &Node) -> errors::Result<Option<Decision>> {
        let mut _notes = Vec::<NoteEnum>::new();
        let request = if let Some(request) = node.state.get_next_request()? {
            request
        } else {
            return Ok(None);
        };

        let _iterator = self.get_iterator(node, request.pkg.name());
        for (pkg, builds) in &_iterator {
            let mut compat = request.is_version_applicable(&pkg.version);
            if !&compat {
                _iterator.set_builds(&pkg.version, &BuildIterator::EmptyBuildIterator);
                _notes.push(NoteEnum::SkipPackageNote(SkipPackageNote::new(
                    pkg.clone(),
                    compat,
                )));
                continue;
            }

            // XXX is this isinstance possible to be true?
            /* if !isinstance(builds, SortedBuildIterator): */
            let builds = BuildIterator::SortedBuildIterator(SortedBuildIterator::new(
                &node.state.get_option_map(),
                &builds,
            ));
            _iterator.set_builds(&pkg.version, &builds);

            for (spec, repo) in builds {
                // Needed by unreachable code below...
                // let mut spec = spec;
                let build_from_source = spec.pkg.build == Some(Build::Source)
                    && request.pkg.build != Some(Build::Source);
                if build_from_source {
                    // Currently irrefutable...
                    let PackageSource::Spec(spec) = repo;
                    {
                        _notes.push(NoteEnum::SkipPackageNote(
                            SkipPackageNote::new_from_message(
                                spec.pkg.clone(),
                                "cannot build embedded source package",
                            ),
                        ));
                        continue;
                    }
                    /*
                    // Currently unreachable...
                    // FIXME: This should only match `PackageNotFoundError`
                    match repo.read_spec(&spec.pkg.with_build(None)) {
                        Ok(s) => spec = s,
                        Err(_) => {
                            _notes.push(NoteEnum::SkipPackageNote(
                                SkipPackageNote::new_from_message(
                                    &spec.pkg,
                                    "cannot build from source, version spec not available",
                                ),
                            ));
                            continue;
                        }
                    }
                    */
                }

                compat = self.validate(&node.state, &spec);
                if !&compat {
                    _notes.push(NoteEnum::SkipPackageNote(SkipPackageNote::new(
                        spec.pkg, compat,
                    )));
                    continue;
                }

                let mut decision = if build_from_source {
                    match self.resolve_new_build(&spec, &node.state) {
                        Ok(build_env) => Decision::build_package(&spec, &repo, &build_env),

                        // FIXME: This should only match `SolverError`
                        Err(err) => {
                            _notes.push(NoteEnum::SkipPackageNote(
                                SkipPackageNote::new_from_message(
                                    spec.pkg,
                                    &format!("cannot resolve build env: {:?}", err),
                                ),
                            ));
                            continue;
                        }
                    }
                } else {
                    Decision::resolve_package(&spec, &repo)
                };
                decision.add_notes(_notes.iter());
                return Ok(Some(decision));
            }
        }

        Err(errors::Error::OutOfOptions(errors::OutOfOptions {
            request,
            notes: _notes,
        }))
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
            validators: validation::default_validators(),
            last_graph: Arc::new(RwLock::new(Graph::new())),
        }
    }

    /// Add a repository where the solver can get packages.
    pub fn add_repository(&mut self, repo: PyObject) {
        self.repos.push(repo);
    }

    /// Add a request to this solver.
    pub fn add_request(&mut self, request: RequestEnum) -> PyResult<()> {
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
                    break Changes::RequestPackage(RequestPackage { request });
                }
                RequestEnum::Request(request) => match request {
                    Request::Pkg(request) => {
                        break Changes::RequestPackage(RequestPackage { request })
                    }
                    Request::Var(request) => break Changes::RequestVar(RequestVar { request }),
                },
            }
        };
        self.initial_state_builders.push(request);
        Ok(())
    }

    pub fn get_initial_state(&self) -> State {
        let mut state = State::default();
        for change in self.initial_state_builders.iter() {
            state = change.apply(&state)
        }
        state
    }

    pub fn reset(&mut self) {
        self.repos.clear();
        self.initial_state_builders.clear();
        self.validators = validation::default_validators();
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
                .insert(0, Validators::BinaryOnly(BinaryOnlyValidator {}))
        } else {
            // Remove all BinaryOnly validators because one was found.
            self.validators = self
                .validators
                .iter()
                .filter(|v| !matches!(v, Validators::BinaryOnly(_)))
                .copied()
                .collect();
        }
    }

    pub fn solve(&mut self) -> PyResult<Solution> {
        let solve_graph = Arc::new(RwLock::new(Graph::new()));
        self.last_graph = solve_graph.clone();

        let mut history = Vec::<Arc<RwLock<Node>>>::new();
        let mut current_node: Option<Arc<RwLock<Node>>> = None;
        let mut decision = Some(Decision::new(self.initial_state_builders.clone()));

        while decision.is_some()
            && (current_node.is_none()
                || !current_node
                    .as_ref()
                    .map(|n| std::ptr::eq(&n.read().unwrap().state, &*DEAD_STATE))
                    .unwrap_or_default())
        {
            // The python code would `yield (current_node, decision)` here,
            // capturing the yielded value into SolverRuntime.solution.

            current_node = Some({
                let mut sg = solve_graph.write().unwrap();
                let root_id = sg.root.read().unwrap().id();
                match sg.add_branch(
                    current_node
                        .as_ref()
                        .map(|n| n.read().unwrap().id())
                        .unwrap_or(root_id),
                    &decision.unwrap(),
                ) {
                    Ok(cn) => cn,
                    Err(GraphError::RecursionError(msg)) => {
                        match history.pop() {
                            Some(n) => {
                                let n_lock = n.read().unwrap();
                                decision = Some(
                                    Changes::StepBack(StepBack::new(
                                        &msg.to_string(),
                                        &n_lock.state,
                                    ))
                                    .as_decision(),
                                )
                            }
                            None => {
                                decision = Some(
                                    Changes::StepBack(StepBack::new(&msg.to_string(), &DEAD_STATE))
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
            let current_node_lock = current_node.read().unwrap();
            decision = self
                .step_state(&current_node_lock)
                .map_err(|err| -> PyErr { err.into() })?;
            history.push(current_node.clone());
        }

        let current_node = current_node.expect("current_node always `is_some` here");
        let current_node_lock = current_node.read().unwrap();

        let is_dead = current_node_lock.state.id()
            == solve_graph.read().unwrap().root.read().unwrap().state.id()
            || std::ptr::eq(&current_node_lock.state, &*DEAD_STATE);
        let is_empty = self.get_initial_state().get_pkg_requests().is_empty();
        if is_dead && !is_empty {
            Err(SolverFailedError::new_err(
                (*solve_graph).read().unwrap().clone(),
            ))
        } else {
            Ok(current_node_lock.state.as_solution()?)
        }
    }

    pub fn update_options(&mut self, options: OptionMap) {
        self.initial_state_builders
            .push(Changes::SetOptions(graph::SetOptions::new(options)))
    }

    fn validate(&self, _node: &State, _spec: &api::Spec) -> api::Compatibility {
        todo!()
    }
}
