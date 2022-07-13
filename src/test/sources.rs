// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use super::TestError;
use crate::{api, build, exec, solve, storage, Result};

pub struct PackageSourceTester<'a> {
    prefix: PathBuf,
    spec: api::Spec,
    script: String,
    repos: Vec<Arc<storage::RepositoryHandle>>,
    options: api::OptionMap,
    additional_requirements: Vec<api::Request>,
    source: Option<PathBuf>,
    env_resolver: crate::BoxedResolverCallback<'a>,
    last_solve_graph: Arc<tokio::sync::RwLock<solve::Graph>>,
}

impl<'a> PackageSourceTester<'a> {
    pub fn new(spec: api::Spec, script: String) -> Self {
        Self {
            prefix: PathBuf::from("/spfs"),
            spec,
            script,
            repos: Vec::new(),
            options: api::OptionMap::default(),
            additional_requirements: Vec::new(),
            source: None,
            env_resolver: Box::new(crate::DefaultResolver {}),
            last_solve_graph: Arc::new(tokio::sync::RwLock::new(solve::Graph::new())),
        }
    }

    pub fn with_option(
        &mut self,
        name: impl Into<api::OptNameBuf>,
        value: impl Into<String>,
    ) -> &mut Self {
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

    /// Setting the source path for this test will validate this
    /// local path rather than a source package's contents.
    pub fn with_source(&mut self, source: Option<PathBuf>) -> &mut Self {
        self.source = source;
        self
    }

    /// Specify additional requirements for the test environment
    pub fn with_requirements(
        &mut self,
        requests: impl IntoIterator<Item = api::Request>,
    ) -> &mut Self {
        self.additional_requirements.extend(requests);
        self
    }

    /// Provide a function that will be called when resolving the test environment.
    ///
    /// This function should run the provided solver runtime to
    /// completion, returning the final result. This function
    /// is useful for introspecting and reporting on the solve
    /// process as needed.
    pub fn watch_environment_resolve<F>(&mut self, resolver: F) -> &mut Self
    where
        F: crate::ResolverCallback + 'a,
    {
        self.env_resolver = Box::new(resolver);
        self
    }

    /// Return the solver graph for the test environment.
    ///
    /// This is most useful for debugging test environments that failed to resolve,
    /// and test that failed with a SolverError.
    ///
    /// If the tester has not run, return an incomplete graph.
    pub fn get_solve_graph(&self) -> Arc<tokio::sync::RwLock<solve::Graph>> {
        self.last_solve_graph.clone()
    }

    /// Execute the source package test as configured.
    pub async fn test(&mut self) -> Result<()> {
        let mut rt = spfs::active_runtime().await?;
        rt.reset_all()?;
        rt.status.editable = true;
        rt.status.stack.clear();

        let mut solver = solve::Solver::default();
        solver.set_binary_only(true);
        for request in self.additional_requirements.drain(..) {
            solver.add_request(request)
        }
        solver.update_options(self.options.clone());
        for repo in self.repos.iter().cloned() {
            solver.add_repository(repo);
        }

        if self.source.is_none() {
            // we only require the source package to actually exist
            // if a local directory has not been specified for the test
            let source_pkg = self.spec.pkg.with_build(Some(api::Build::Source));
            let mut ident_range = api::RangeIdent::exact(&source_pkg, [api::Component::Source]);
            ident_range.components.insert(api::Component::Source);
            let request =
                api::PkgRequest::new(ident_range, api::RequestedBy::SourceTest(source_pkg))
                    .with_prerelease(api::PreReleasePolicy::IncludeAll)
                    .with_pin(None)
                    .with_compat(None);
            solver.add_request(request.into());
        }

        let mut runtime = solver.run();
        let solution = self.env_resolver.solve(&mut runtime).await;
        self.last_solve_graph = runtime.graph();
        let solution = solution?;

        for layer in exec::resolve_runtime_layers(&solution).await? {
            rt.push_digest(layer);
        }
        rt.save_state_to_storage().await?;
        spfs::remount_runtime(&rt).await?;

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
            Some(source) => source.clone(),
            None => build::source_package_path(&self.spec.pkg.with_build(Some(api::Build::Source)))
                .to_path(&self.prefix),
        };

        let tmpdir = tempfile::Builder::new().prefix("spk-test").tempdir()?;
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
}
