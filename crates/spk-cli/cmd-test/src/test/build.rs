// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::convert::TryInto;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use spk_build::{source_package_path, BuildSource};
use spk_cli_common::Result;
use spk_exec::resolve_runtime_layers;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::ident::{PkgRequest, PreReleasePolicy, RangeIdent, Request, RequestedBy};
use spk_schema::{AnyIdent, Recipe, SpecRecipe};
use spk_solve::graph::Graph;
use spk_solve::solution::Solution;
use spk_solve::{BoxedResolverCallback, DefaultResolver, ResolverCallback, Solver};
use spk_storage::{self as storage};

use super::Tester;

pub struct PackageBuildTester<'a> {
    prefix: PathBuf,
    recipe: SpecRecipe,
    script: String,
    repos: Vec<Arc<storage::RepositoryHandle>>,
    options: OptionMap,
    additional_requirements: Vec<Request>,
    source: BuildSource,
    source_resolver: BoxedResolverCallback<'a>,
    build_resolver: BoxedResolverCallback<'a>,
    last_solve_graph: Arc<tokio::sync::RwLock<Graph>>,
}

impl<'a> PackageBuildTester<'a> {
    pub fn new(recipe: SpecRecipe, script: String) -> Self {
        let source = BuildSource::SourcePackage(recipe.ident().to_any(Some(Build::Source)).into());
        Self {
            prefix: PathBuf::from("/spfs"),
            recipe,
            script,
            repos: Vec::new(),
            options: OptionMap::default(),
            additional_requirements: Vec::new(),
            source,
            source_resolver: Box::new(DefaultResolver {}),
            build_resolver: Box::new(DefaultResolver {}),
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

    /// Setting the source determines whether the script runs in
    /// the root of an existing source package or a local directory.
    pub fn with_source(&mut self, source: BuildSource) -> &mut Self {
        self.source = source;
        self
    }

    pub fn with_requirements(&mut self, requests: impl IntoIterator<Item = Request>) -> &mut Self {
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
        F: ResolverCallback + 'a,
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
        F: ResolverCallback + 'a,
    {
        self.build_resolver = Box::new(resolver);
        self
    }

    pub async fn test(&mut self) -> Result<()> {
        let mut rt = spfs::active_runtime().await?;
        rt.reset_all()?;
        rt.status.editable = true;
        rt.status.stack.clear();

        if let BuildSource::SourcePackage(pkg) = self.source.clone() {
            let solution = self.resolve_source_package(&pkg.try_into()?).await?;
            for layer in resolve_runtime_layers(&solution).await? {
                rt.push_digest(layer);
            }
        }

        let mut solver = Solver::default();
        solver.set_binary_only(true);
        solver.update_options(self.options.clone());
        for repo in self.repos.iter().cloned() {
            solver.add_repository(repo);
        }
        solver.configure_for_build_environment(&self.recipe)?;
        for request in self.additional_requirements.drain(..) {
            solver.add_request(request)
        }
        let mut runtime = solver.run();
        let solution = self.build_resolver.solve(&mut runtime).await;
        self.last_solve_graph = runtime.graph();
        let solution = solution?;

        for layer in resolve_runtime_layers(&solution).await? {
            rt.push_digest(layer);
        }
        rt.save_state_to_storage().await?;
        spfs::remount_runtime(&rt).await?;

        self.options.extend(solution.options().clone());
        let _spec = self.recipe.generate_binary_build(&solution)?;

        let env = solution.to_environment(Some(std::env::vars()));

        let source_dir = match &self.source {
            BuildSource::SourcePackage(source) => {
                source_package_path(&source.try_into()?).to_path(&self.prefix)
            }
            BuildSource::LocalPath(path) => path.clone(),
        };

        self.execute_test_script(&source_dir, env, &rt)
    }

    async fn resolve_source_package(&mut self, package: &AnyIdent) -> Result<Solution> {
        let mut solver = Solver::default();
        solver.update_options(self.options.clone());
        let local_repo: Arc<storage::RepositoryHandle> =
            Arc::new(storage::local_repository().await?.into());
        solver.add_repository(local_repo.clone());
        for repo in self.repos.iter() {
            if **repo == *local_repo {
                // local repo is always injected first, and duplicates are redundant
                continue;
            }
            solver.add_repository(repo.clone());
        }

        let ident_range = RangeIdent::equals(package, [Component::Source]);
        let request = PkgRequest::new(ident_range, RequestedBy::BuildTest(package.clone()))
            .with_prerelease(PreReleasePolicy::IncludeAll)
            .with_pin(None)
            .with_compat(None);

        solver.add_request(request.into());

        let mut runtime = solver.run();
        let solution = self.source_resolver.solve(&mut runtime).await;
        self.last_solve_graph = runtime.graph();
        Ok(solution?)
    }
}

#[async_trait::async_trait]
impl<'a> Tester for PackageBuildTester<'a> {
    async fn test(&mut self) -> Result<()> {
        self.test().await
    }
    fn prefix(&self) -> &Path {
        &self.prefix
    }
    fn script(&self) -> &String {
        &self.script
    }
}
