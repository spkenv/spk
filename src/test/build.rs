// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::{
    api,
    build::{self, BuildSource},
    exec, solve, storage, Result,
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

pub struct PackageBuildTester {
    prefix: PathBuf,
    spec: api::Spec,
    script: String,
    repos: Vec<Arc<storage::RepositoryHandle>>,
    options: api::OptionMap,
    additional_requirements: Vec<api::Request>,
    source: BuildSource,
    source_resolver: Box<dyn FnMut(&mut solve::SolverRuntime) -> Result<solve::Solution>>,
    build_resolver: Box<dyn FnMut(&mut solve::SolverRuntime) -> Result<solve::Solution>>,
    last_solve_graph: Arc<RwLock<solve::Graph>>,
}

impl PackageBuildTester {
    pub fn new(spec: api::Spec, script: String) -> Self {
        let source = BuildSource::SourcePackage(spec.pkg.with_build(Some(api::Build::Source)));
        Self {
            prefix: PathBuf::from("/spfs"),
            spec,
            script,
            repos: Vec::new(),
            options: api::OptionMap::default(),
            additional_requirements: Vec::new(),
            source,
            source_resolver: Box::new(|r| r.solution()),
            build_resolver: Box::new(|r| r.solution()),
            last_solve_graph: Arc::new(RwLock::new(solve::Graph::new())),
        }
    }

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
        repos: impl IntoIterator<Item = Arc<storage::RepositoryHandle>>,
    ) -> &mut Self {
        self.repos.extend(repos.into_iter());
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

    /// Provide a function that will be called when resolving the source package.
    ///
    /// This function should run the provided solver runtime to
    /// completion, returning the final result. This function
    /// is useful for introspecting and reporting on the solve
    /// process as needed.
    pub fn with_source_resolver<F>(&mut self, resolver: F) -> &mut Self
    where
        F: FnMut(&mut solve::SolverRuntime) -> Result<solve::Solution> + 'static,
    {
        self.source_resolver = Box::new(resolver);
        self
    }

    /// Provide a function that will be called when resolving the build environment.
    ///
    /// This function should run the provided solver runtime to
    /// completion, returning the final result. This function
    /// is useful for introspecting and reporting on the solve
    /// process as needed.
    pub fn with_build_resolver<F>(&mut self, resolver: F) -> &mut Self
    where
        F: FnMut(&mut solve::SolverRuntime) -> Result<solve::Solution> + 'static,
    {
        self.build_resolver = Box::new(resolver);
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

    pub fn test(&mut self) -> Result<()> {
        let _guard = crate::HANDLE.enter();
        let mut rt = crate::HANDLE.block_on(spfs::active_runtime())?;
        rt.reset_all()?;
        rt.status.editable = true;
        rt.status.stack.clear();

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
        self.last_solve_graph = runtime.graph();
        let solution = (self.build_resolver)(&mut runtime)?;

        for layer in exec::resolve_runtime_layers(&solution)? {
            rt.push_digest(layer);
        }
        crate::HANDLE.block_on(async {
            rt.save_state_to_storage().await?;
            spfs::remount_runtime(&rt).await
        })?;

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
        let cmd = spfs::build_shell_initialized_command(
            &rt,
            OsString::from("bash"),
            &[OsString::from("-ex"), script_path.into_os_string()],
        )?;
        let mut cmd = cmd.into_std();
        let status = cmd.envs(env).current_dir(source_dir).status()?;
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

        let ident_range = api::RangeIdent::exact(package, [api::Component::Source]);
        let request = api::PkgRequest {
            pkg: ident_range,
            prerelease_policy: api::PreReleasePolicy::IncludeAll,
            inclusion_policy: api::InclusionPolicy::Always,
            pin: None,
            required_compat: None,
        };
        solver.add_request(request.into());

        let mut runtime = solver.run();
        let solution = (self.source_resolver)(&mut runtime);
        self.last_solve_graph = runtime.graph();
        solution
    }
}
