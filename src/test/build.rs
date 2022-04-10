// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use pyo3::prelude::*;

use crate::build::BuildVariant;
use crate::{
    api,
    build::{self, BuildSource},
    exec, solve, storage, Error, Result,
};

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
    spec: api::SpecWithBuildVariant,
    script: String,
    repos: Vec<Arc<storage::RepositoryHandle>>,
    options: api::OptionMap,
    additional_requirements: Vec<api::Request>,
    source: BuildSource,
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
        self.repos.push(Arc::new(repo));
        self
    }

    pub fn with_repositories(
        &mut self,
        repos: impl IntoIterator<Item = storage::RepositoryHandle>,
    ) -> &mut Self {
        self.repos.extend(repos.into_iter().map(Arc::new));
        self
    }

    /// Setting the source determines whether the script runs in
    /// the root of an existing source package or a local directory.
    pub fn with_source(&mut self, source: BuildSource) -> &mut Self {
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
        let source = BuildSource::SourcePackage(spec.pkg.with_build(Some(api::Build::Source)));
        Self {
            prefix: PathBuf::from("/spfs"),
            spec: api::SpecWithBuildVariant {
                spec: Arc::new(spec),
                variant: BuildVariant::Default,
            },
            script,
            repos: Vec::new(),
            options: api::OptionMap::default(),
            additional_requirements: Vec::new(),
            source,
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
    fn with_source_py(mut slf: PyRefMut<Self>, source: Py<PyAny>) -> Result<PyRefMut<Self>> {
        if let Ok(ident) = source.extract::<api::Ident>(slf.py()) {
            slf.with_source(BuildSource::SourcePackage(ident));
        } else if let Ok(path) = source.extract::<String>(slf.py()) {
            slf.with_source(BuildSource::LocalPath(path.into()));
        } else {
            return Err(Error::String("Expected api.Ident or str".to_string()));
        }
        Ok(slf)
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
        let _guard = crate::HANDLE.enter();
        let mut rt = spfs::active_runtime()?;
        rt.set_editable(true)?;
        rt.reset_all()?;
        rt.reset_stack()?;

        let mut stack = Vec::new();
        if let BuildSource::SourcePackage(pkg) = self.source.clone() {
            let solution = self.resolve_source_package(&pkg)?;
            stack.append(&mut exec::resolve_runtime_layers(&solution)?);
        }

        let mut solver = solve::Solver::default();
        solver.set_binary_only(true);
        for request in self.additional_requirements.drain(..) {
            solver.add_request(request)
        }
        solver.update_options(self.options.clone());
        for repo in self.repos.iter().cloned() {
            solver.add_repository(repo);
        }
        solver.configure_for_build_environment(&self.spec)?;
        let mut runtime = solver.run();
        let result = runtime.solution();
        self.last_solve_graph = runtime.graph();
        let solution = result?;

        for layer in exec::resolve_runtime_layers(&solution)? {
            rt.push_digest(&layer)?;
        }
        crate::HANDLE.block_on(spfs::remount_runtime(&rt))?;

        self.options.extend(solution.options());
        let resolved = solution.items().into_iter().map(|r| r.spec);
        self.spec.update_for_build(&self.options, resolved)?;

        let mut env = solution.to_environment(Some(std::env::vars()));
        env.insert(
            "PREFIX".to_string(),
            self.prefix
                .to_str()
                .ok_or_else(|| {
                    crate::Error::String("Test prefix must be a valid unicode string".to_string())
                })?
                .to_string(),
        );

        let source_dir = match &self.source {
            BuildSource::SourcePackage(source) => {
                build::source_package_path(source).to_path(&self.prefix)
            }
            BuildSource::LocalPath(path) => path.clone(),
        };

        let tmpdir = tempdir::TempDir::new("spk-test")?;
        let script_path = tmpdir.path().join("test.sh");
        let mut script_file = std::fs::File::create(&script_path)?;
        script_file.write_all(self.script.as_bytes())?;
        script_file.sync_data()?;
        // TODO: this should be more easily configurable on the spfs side
        std::env::set_var("SHELL", "bash");
        let args = spfs::build_shell_initialized_command(
            OsString::from("bash"),
            &mut vec![OsString::from("-ex"), script_path.into_os_string()],
        )?;
        let mut cmd = std::process::Command::new(args.get(0).unwrap());
        let status = cmd
            .args(&args[1..])
            .envs(env)
            .current_dir(source_dir)
            .status()?;
        if !status.success() {
            Err(TestError::new_error(format!(
                "Test script returned non-zero exit status: {}",
                status.code().unwrap_or(1)
            )))
        } else {
            Ok(())
        }
    }

    fn resolve_source_package(&mut self, package: &api::Ident) -> Result<solve::Solution> {
        let mut solver = solve::Solver::default();
        solver.set_binary_only(true);
        solver.update_options(self.options.clone());
        let local_repo: Arc<storage::RepositoryHandle> =
            Arc::new(crate::HANDLE.block_on(storage::local_repository())?.into());
        solver.add_repository(local_repo.clone());
        for repo in self.repos.iter() {
            if **repo == *local_repo {
                // local repo is always injected first, and duplicates are redundant
                continue;
            }
            solver.add_repository(repo.clone());
        }

        let mut ident_range = api::RangeIdent::exact(package, [api::Component::All]);
        ident_range.components.insert(api::Component::Source);
        let request = api::PkgRequest {
            pkg: ident_range,
            prerelease_policy: api::PreReleasePolicy::IncludeAll,
            inclusion_policy: api::InclusionPolicy::Always,
            pin: None,
            required_compat: None,
        };
        solver.add_request(request.into());

        let mut runtime = solver.run();
        let solution = runtime.solution();
        self.last_solve_graph = runtime.graph();
        Ok(solution?)
    }
}
