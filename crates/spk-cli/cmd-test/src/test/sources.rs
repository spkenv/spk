// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::{Path, PathBuf};
use std::sync::Arc;

use spk_build::source_package_path;
use spk_cli_common::Result;
use spk_exec::resolve_runtime_layers;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::ident::{PkgRequest, PreReleasePolicy, RangeIdent, Request, RequestedBy};
use spk_schema::{Recipe, SpecRecipe};
use spk_solve::{DecisionFormatter, SolverExt, SolverMut};
use spk_storage as storage;

use super::Tester;

pub struct PackageSourceTester<Solver>
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
    source: Option<PathBuf>,
    env_formatter: DecisionFormatter,
}

impl<Solver> PackageSourceTester<Solver>
where
    Solver: SolverExt + SolverMut + Send,
{
    pub fn new(recipe: SpecRecipe, script: String, solver: Solver) -> Self {
        Self {
            prefix: PathBuf::from("/spfs"),
            recipe,
            script,
            repos: Vec::new(),
            solver,
            options: OptionMap::default(),
            additional_requirements: Vec::new(),
            source: None,
            env_formatter: DecisionFormatter::default(),
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

    /// Provide a formatter to use when resolving the test environment.
    pub fn watch_environment_formatter(&mut self, formatter: DecisionFormatter) -> &mut Self {
        self.env_formatter = formatter;
        self
    }

    /// Execute the source package test as configured.
    pub async fn test(&mut self) -> Result<()> {
        let mut rt = spfs::active_runtime().await?;
        rt.reset_all()?;
        rt.status.editable = true;
        rt.status.stack.clear();

        let requires_localization = rt.config.mount_backend.requires_localization();

        self.solver.set_binary_only(true);
        self.solver.update_options(self.options.clone());
        for repo in self.repos.iter().cloned() {
            self.solver.add_repository(repo);
        }

        if self.source.is_none() {
            // we only require the source package to actually exist
            // if a local directory has not been specified for the test
            let source_pkg = self.recipe.ident().to_any_ident(Some(Build::Source));
            let mut ident_range = RangeIdent::equals(&source_pkg, [Component::Source]);
            ident_range.components.insert(Component::Source);
            let request = PkgRequest::new(ident_range, RequestedBy::SourceTest(source_pkg))
                .with_prerelease(Some(PreReleasePolicy::IncludeAll))
                .with_pin(None)
                .with_compat(None);
            self.solver.add_request(request.into());
        }

        for request in self.additional_requirements.drain(..) {
            self.solver.add_request(request)
        }

        // let (solution, _) = self.env_resolver.solve(&solver).await?;
        let solution = self
            .solver
            .run_and_print_resolve(&self.env_formatter)
            .await?;

        for layer in resolve_runtime_layers(requires_localization, &solution).await? {
            rt.push_digest(layer);
        }
        rt.save_state_to_storage().await?;
        spfs::remount_runtime(&rt).await?;

        let env = solution.to_environment(Some(std::env::vars()));

        let source_dir = match &self.source {
            Some(source) => source.clone(),
            None => source_package_path(&self.recipe.ident().to_build_ident(Build::Source))
                .to_path(&self.prefix),
        };

        self.execute_test_script(&source_dir, env, &rt)
    }
}

#[async_trait::async_trait]
impl<Solver> Tester for PackageSourceTester<Solver>
where
    Solver: SolverExt + SolverMut + Send,
{
    async fn test(&mut self) -> Result<()> {
        PackageSourceTester::test(self).await
    }
    fn prefix(&self) -> &Path {
        &self.prefix
    }
    fn script(&self) -> &String {
        &self.script
    }
}
