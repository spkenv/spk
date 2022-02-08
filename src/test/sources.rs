// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::PathBuf;

use pyo3::prelude::*;

use crate::{api, solve, storage, Result};

#[pyclass]
pub struct PackageSourceTester {
    prefix: PathBuf,
    spec: api::Spec,
    script: String,
    repos: Vec<storage::RepositoryHandle>,
    options: api::OptionMap,
    additional_requirements: Vec<api::Request>,
    source: Option<PathBuf>,
    last_solve_graph: solve::Graph,
}

#[pymethods]
impl PackageSourceTester {
    #[new]
    pub fn new(spec: api::Spec, script: String) -> Self {
        // self._prefix = "/spfs"
        // self._spec = spec
        // self._script = script
        // self._repos: List[storage.Repository] = []
        // self._options = api.OptionMap()
        // self._additional_requirements: List[api.Request] = []
        // self._source: Optional[str] = None
        // self._last_solve_graph = solve.Graph()
        todo!()
    }

    /// Return the solver graph for the test environment.
    ///
    /// This is most useful for debugging test environments that failed to resolve,
    /// and test that failed with a SolverError.
    ///
    /// If the tester has not run, return an incomplete graph.
    fn get_solve_graph(&self) -> &solve::Graph {
        &self.last_solve_graph
    }

    fn with_option(self, name: String, value: String) -> Self {
        self.options.insert(name, value);
        self
    }

    fn with_options(self, options: api::OptionMap) -> Self {
        self.options.append(&mut options);
        self
    }

    fn with_repository(self, repo: storage::python::Repository) -> Self {
        self.repos.push(repo.handle);
        self
    }

    fn with_repositories(self, repos: Vec<storage::python::Repository>) -> Self {
        self.repos.extend(&mut repos.into_iter().map(|r| r.handle));
        self
    }

    fn with_source(self, source: PathBuf) -> Self {
        self.source = source;
        self
    }

    fn with_requirements(self, requests: Vec<api::Request>) -> Self {
        self.additional_requirements.extend(&mut requests);
        self
    }

    pub fn test(&self) -> Result<()> {
        // spkrs.reconfigure_runtime(editable=True, stack=[], reset=["*"])

        // solver = solve.Solver()
        // for request in self._additional_requirements:
        //     solver.add_request(request)
        // solver.update_options(&self._options)
        // for repo in self._repos:
        //     solver.add_repository(repo)
        // solver.add_request(&self._spec.pkg.with_build(api.SRC))
        // runtime = solver.run()
        // try:
        //     solution = runtime.solution()
        // finally:
        //     self._last_solve_graph = runtime.graph()

        // layers = exec.resolve_runtime_layers(solution)
        // spkrs.reconfigure_runtime(stack=layers)

        // env = solution.to_environment(os.environ)
        // env["PREFIX"] = self._prefix

        // if self._source is not None:
        //     source_dir = self._source
        // else:
        //     source_dir = build.source_package_path(
        //         self._spec.pkg.with_build(api.SRC), self._prefix
        //     )
        // with tempfile.NamedTemporaryFile("w+") as script_file:
        //     script_file.write(&self._script)
        //     script_file.flush()
        //     os.environ["SHELL"] = "bash"
        //     cmd = spkrs.build_shell_initialized_command("bash", "-ex", script_file.name)

        //     with build.deferred_signals():
        //         proc = subprocess.Popen(cmd, cwd=source_dir, env=env)
        //         proc.wait()
        //     if proc.returncode != 0:
        //         raise TestError(
        //             f"Test script returned non-zero exit status: {proc.returncode}"
        //         )
        todo!()
    }
}
