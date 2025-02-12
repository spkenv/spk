// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryInto;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use spk_build::{BuildSource, source_package_path};
use spk_cli_common::Result;
use spk_exec::resolve_runtime_layers;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::ident::{PkgRequest, PreReleasePolicy, RangeIdent, Request, RequestedBy};
use spk_schema::{AnyIdent, Recipe, SpecRecipe};
use spk_solve::solution::Solution;
use spk_solve::{DecisionFormatter, SolverExt, SolverMut};
use spk_storage as storage;

use super::Tester;

pub struct PackageBuildTester<Solver>
where
    Solver: Send,
{
    prefix: PathBuf,
    recipe: SpecRecipe,
    script: String,
    repos: Vec<Arc<storage::RepositoryHandle>>,
    solver: Solver,
    options: OptionMap,
    additional_requirements: Vec<Request>,
    source: BuildSource,
    source_formatter: DecisionFormatter,
    build_formatter: DecisionFormatter,
}

impl<Solver> PackageBuildTester<Solver>
where
    Solver: SolverExt + SolverMut + Default + Send,
{
    pub fn new(recipe: SpecRecipe, script: String, solver: Solver) -> Self {
        let source =
            BuildSource::SourcePackage(recipe.ident().to_any_ident(Some(Build::Source)).into());
        Self {
            prefix: PathBuf::from("/spfs"),
            recipe,
            script,
            repos: Vec::new(),
            solver,
            options: OptionMap::default(),
            additional_requirements: Vec::new(),
            source,
            source_formatter: DecisionFormatter::default(),
            build_formatter: DecisionFormatter::default(),
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
        self.repos.extend(repos);
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

    /// Provide a formatter to use when resolving the source package.
    pub fn with_source_formatter(&mut self, formatter: DecisionFormatter) -> &mut Self {
        self.source_formatter = formatter;
        self
    }

    /// Provide a formatter to use when resolving the build environment.
    pub fn with_build_formatter(&mut self, formatter: DecisionFormatter) -> &mut Self {
        self.build_formatter = formatter;
        self
    }

    pub async fn test(&mut self) -> Result<()> {
        let mut rt = spfs::active_runtime().await?;
        rt.reset_all()?;
        rt.status.editable = true;
        rt.status.stack.clear();

        let requires_localization = rt.config.mount_backend.requires_localization();

        if let BuildSource::SourcePackage(pkg) = self.source.clone() {
            let solution = self.resolve_source_package(&pkg.try_into()?).await?;
            for layer in resolve_runtime_layers(requires_localization, &solution).await? {
                rt.push_digest(layer);
            }
        }

        self.solver.set_binary_only(true);
        self.solver.update_options(self.options.clone());
        for repo in self.repos.iter().cloned() {
            self.solver.add_repository(repo);
        }
        // TODO
        // solver.configure_for_build_environment(&self.recipe)?;
        for request in self.additional_requirements.drain(..) {
            self.solver.add_request(request)
        }

        // let (solution, _) = self.build_resolver.solve(&solver).await?;
        let solution = self
            .solver
            .run_and_print_resolve(&self.build_formatter)
            .await?;

        for layer in resolve_runtime_layers(requires_localization, &solution).await? {
            rt.push_digest(layer);
        }
        rt.save_state_to_storage().await?;
        spfs::remount_runtime(&rt).await?;

        self.options.extend(solution.options().clone());
        let _spec = self
            .recipe
            .generate_binary_build(&self.options, &solution)?;

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
            .with_prerelease(Some(PreReleasePolicy::IncludeAll))
            .with_pin(None)
            .with_compat(None);

        solver.add_request(request.into());

        // let (solution, _) = self.source_resolver.solve(&solver).await?;
        let solution = solver.run_and_print_resolve(&self.source_formatter).await?;
        Ok(solution)
    }
}

#[async_trait::async_trait]
impl<Solver> Tester for PackageBuildTester<Solver>
where
    Solver: SolverExt + SolverMut + Default + Send,
{
    async fn test(&mut self) -> Result<()> {
        PackageBuildTester::test(self).await
    }
    fn prefix(&self) -> &Path {
        &self.prefix
    }
    fn script(&self) -> &String {
        &self.script
    }
}
