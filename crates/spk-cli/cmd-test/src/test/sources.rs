// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use spk_build::source_package_path;
use spk_cli_common::{Error, Result, TestError};
use spk_exec::resolve_runtime_layers;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::spec_ops::RecipeOps;
use spk_schema::ident::{PkgRequest, PreReleasePolicy, RangeIdent, Request, RequestedBy};
use spk_schema::SpecRecipe;
use spk_solve::graph::Graph;
use spk_solve::{BoxedResolverCallback, DefaultResolver, ResolverCallback, Solver};
use spk_storage::{self as storage};

pub struct PackageSourceTester<'a> {
    prefix: PathBuf,
    recipe: SpecRecipe,
    script: String,
    repos: Vec<Arc<storage::RepositoryHandle>>,
    options: OptionMap,
    additional_requirements: Vec<Request>,
    source: Option<PathBuf>,
    env_resolver: BoxedResolverCallback<'a>,
    last_solve_graph: Arc<tokio::sync::RwLock<Graph>>,
}

impl<'a> PackageSourceTester<'a> {
    pub fn new(recipe: SpecRecipe, script: String) -> Self {
        Self {
            prefix: PathBuf::from("/spfs"),
            recipe,
            script,
            repos: Vec::new(),
            options: OptionMap::default(),
            additional_requirements: Vec::new(),
            source: None,
            env_resolver: Box::new(DefaultResolver {}),
            last_solve_graph: Arc::new(tokio::sync::RwLock::new(Graph::new())),
        }
    }

    pub fn with_options(&mut self, mut options: OptionMap) -> &mut Self {
        self.options.append(&mut options);
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
    pub fn with_requirements(&mut self, requests: impl IntoIterator<Item = Request>) -> &mut Self {
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
        F: ResolverCallback + 'a,
    {
        self.env_resolver = Box::new(resolver);
        self
    }

    /// Execute the source package test as configured.
    pub async fn test(&mut self) -> Result<()> {
        let mut rt = spfs::active_runtime().await?;
        rt.reset_all()?;
        rt.status.editable = true;
        rt.status.stack.clear();

        let mut solver = Solver::default();
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
            let source_pkg = self.recipe.to_ident().into_build(Build::Source);
            let mut ident_range = RangeIdent::equals(&source_pkg, [Component::Source]);
            ident_range.components.insert(Component::Source);
            let request = PkgRequest::new(ident_range, RequestedBy::SourceTest(source_pkg))
                .with_prerelease(PreReleasePolicy::IncludeAll)
                .with_pin(None)
                .with_compat(None);
            solver.add_request(request.into());
        }

        let mut runtime = solver.run();
        let solution = self.env_resolver.solve(&mut runtime).await;
        self.last_solve_graph = runtime.graph();
        let solution = solution?;

        for layer in resolve_runtime_layers(&solution).await? {
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
                    Error::String("Test prefix must be a valid unicode string".to_string())
                })?
                .to_string(),
        );

        let source_dir = match &self.source {
            Some(source) => source.clone(),
            None => source_package_path(&self.recipe.to_ident().into_build(Build::Source))
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
