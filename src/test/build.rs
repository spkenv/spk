// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use pyo3::prelude::*;

use crate::{api, solve, storage, Result};

/// Denotes that a test has failed or was invalid.
#[derive(Debug)]
pub struct TestError {
    pub message: String,
}

impl TestError {
    pub fn new_error(msg: String) -> crate::Error {
        crate::Error::Test(Self { message: msg })
    }
}

#[pyclass]
pub struct PackageBuildTester {
    prefix: PathBuf,
    spec: api::Spec,
    script: String,
    repos: Vec<Arc<Mutex<storage::RepositoryHandle>>>,
    options: api::OptionMap,
    additional_requirements: Vec<api::Request>,
    source: Option<PathBuf>,
    last_solve_graph: Arc<RwLock<solve::Graph>>,
}

impl PackageBuildTester {
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
impl PackageBuildTester {
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

    pub fn test(&self) -> Result<()> {
        // spkrs.reconfigure_runtime(editable=True, reset=["*"], stack=[])

        // solution = self._resolve_source_package()
        // stack = exec.resolve_runtime_layers(solution)
        // spkrs.reconfigure_runtime(stack=stack)

        // solver = solve.Solver()
        // for request in self._additional_requirements:
        //     solver.add_request(request)
        // solver.update_options(self._options)
        // for repo in self._repos:
        //     solver.add_repository(repo)
        // if isinstance(self._source, api.Ident):
        //     ident_range = api.parse_ident_range(
        //         f"{self._source.name}/={self._source.version}/{self._source.build}"
        //     )
        //     request = api.PkgRequest(ident_range, "IncludeAll")
        //     solver.add_request(request)
        // solver.configure_for_build_environment(self._spec)
        // runtime = solver.run()
        // try:
        //     solution = runtime.solution()
        // finally:
        //     self._last_solve_graph = runtime.graph()

        // stack = exec.resolve_runtime_layers(solution)
        // spkrs.reconfigure_runtime(stack=stack)

        // specs = list(s for _, s, _ in solution.items())
        // self._options.update(solution.options())
        // self._spec.update_spec_for_build(self._options, specs)

        // env = solution.to_environment(os.environ)
        // env.update(self._spec.resolve_all_options(solution.options()).to_environment())
        // env.update(build.get_package_build_env(self._spec))
        // env["PREFIX"] = self._prefix

        // source_dir = build.source_package_path(
        //     self._spec.pkg.with_build(api.SRC), self._prefix
        // )
        // with tempfile.NamedTemporaryFile("w+") as script_file:
        //     script_file.write(self._script)
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

    fn resolve_source_package(&self) -> Result<solve::Solution> {
        // solver = solve.Solver()
        // solver.update_options(self._options)
        // solver.add_repository(storage.local_repository())
        // for repo in self._repos:
        //     if repo == storage.local_repository():
        //         # local repo is always injected first, and duplicates are redundant
        //         continue
        //     solver.add_repository(repo)

        // if isinstance(self._source, api.Ident):
        //     ident_range = api.parse_ident_range(
        //         f"{self._source.name}/={self._source.version}/{self._source.build}"
        //     )
        //     request = api.PkgRequest(ident_range, "IncludeAll")
        //     solver.add_request(request)

        // runtime = solver.run()
        // try:
        //     return runtime.solution()
        // finally:
        //     self._last_solve_graph = runtime.graph()
        todo!()
    }
}
