// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};

use pyo3::prelude::*;

use crate::{api, solve, storage, Result};

#[pyclass]
pub struct PackageInstallTester {
    prefix: PathBuf,
    spec: api::Spec,
    script: String,
    repos: Vec<Arc<Mutex<storage::RepositoryHandle>>>,
    options: api::OptionMap,
    additional_requirements: Vec<api::Request>,
    source: Option<PathBuf>,
    last_solve_graph: Arc<RwLock<solve::Graph>>,
}

impl PackageInstallTester {
    pub fn with_option(&mut self, name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.options.insert(name.into(), value.into());
        self
    }

    pub fn with_options(&mut self, mut options: api::OptionMap) -> &mut Self {
        self.options.append(&mut options);
        self
    }

    pub fn with_repository(&mut self, repo: storage::RepositoryHandle) -> &mut Self {
        self.repos.push(Arc::new(Mutex::new(repo)));
        self
    }

    pub fn with_repositories(
        &mut self,
        repos: impl IntoIterator<Item = storage::RepositoryHandle>,
    ) -> &mut Self {
        self.repos
            .extend(repos.into_iter().map(Mutex::new).map(Arc::new));
        self
    }

    /// Setting the source path for this test will validate this
    /// local path rather than a source package's contents.
    pub fn with_source(&mut self, source: Option<PathBuf>) -> &mut Self {
        self.source = source;
        self
    }

    pub fn with_requirements(
        &mut self,
        requests: impl IntoIterator<Item = api::Request>,
    ) -> &mut Self {
        self.additional_requirements.extend(requests);
        self
    }

    /// Return the solver graph for the test environment.
    ///
    /// This is most useful for debugging test environments that failed to resolve,
    /// and test that failed with a SolverError.
    ///
    /// If the tester has not run, return an incomplete graph.
    pub fn get_solve_graph(&self) -> Arc<RwLock<solve::Graph>> {
        self.last_solve_graph.clone()
    }
}

#[pymethods]
impl PackageInstallTester {
    #[new]
    pub fn new(spec: api::Spec, script: String) -> Self {
        Self {
            prefix: PathBuf::from("/spfs"),
            spec,
            script,
            repos: Vec::new(),
            options: api::OptionMap::default(),
            additional_requirements: Vec::new(),
            source: None,
            last_solve_graph: Arc::new(RwLock::new(solve::Graph::new())),
        }
    }

    #[pyo3(name = "get_solve_graph")]
    fn get_solve_graph_py(&self) -> solve::Graph {
        self.get_solve_graph().read().unwrap().clone()
    }

    #[pyo3(name = "with_option")]
    fn with_option_py(mut slf: PyRefMut<Self>, name: String, value: String) -> PyRefMut<Self> {
        slf.with_option(name, value);
        slf
    }

    #[pyo3(name = "with_options")]
    fn with_options_py(mut slf: PyRefMut<Self>, options: api::OptionMap) -> PyRefMut<Self> {
        slf.with_options(options);
        slf
    }

    #[pyo3(name = "with_repository")]
    fn with_repository_py(
        mut slf: PyRefMut<Self>,
        repo: storage::python::Repository,
    ) -> PyRefMut<Self> {
        slf.repos.push(repo.handle);
        slf
    }

    #[pyo3(name = "with_repositories")]
    fn with_repositories_py(
        mut slf: PyRefMut<Self>,
        repos: Vec<storage::python::Repository>,
    ) -> PyRefMut<Self> {
        slf.repos.extend(&mut repos.into_iter().map(|r| r.handle));
        slf
    }

    #[pyo3(name = "with_source")]
    fn with_source_py(mut slf: PyRefMut<Self>, source: Option<PathBuf>) -> PyRefMut<Self> {
        slf.source = source;
        slf
    }

    #[pyo3(name = "with_requirements")]
    fn with_requirements_py(
        mut slf: PyRefMut<Self>,
        requests: Vec<api::Request>,
    ) -> PyRefMut<Self> {
        slf.additional_requirements.extend(requests);
        slf
    }

    pub fn test(&mut self) -> Result<()> {
        // spkrs.reconfigure_runtime(stack=[], editable=True, reset=["*"])

        // solver = solve.Solver()
        // for request in self._additional_requirements:
        //     solver.add_request(request)
        // solver.update_options(self._options)
        // for repo in self._repos:
        //     solver.add_repository(repo)
        // solver.add_request(self._spec.pkg)
        // solution = solver.solve()

        // layers = exec.resolve_runtime_layers(solution)
        // spkrs.reconfigure_runtime(stack=layers)

        // env = solution.to_environment(os.environ)
        // env["PREFIX"] = self._prefix

        // source_dir = "."
        // if self._source is not None:
        //     source_dir = self._source

        // with tempfile.NamedTemporaryFile("w+") as script_file:
        //     script_file.write(self._script)
        //     script_file.flush()
        //     os.environ["SHELL"] = "bash"
        //     cmd = spkrs.build_shell_initialized_command("bash", "-ex", script_file.name)

        //     with build.deferred_signals():
        //         proc = subprocess.Popen(cmd, env=env, cwd=source_dir)
        //         proc.wait()
        //     if proc.returncode != 0:
        //         raise TestError(
        //             f"Test script returned non-zero exit status: {proc.returncode}"
        //         )
        todo!()
    }
}
