// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::{Path, PathBuf};
use std::sync::Arc;

use spk_cli_common::Result;
use spk_exec::resolve_runtime_layers;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::ident::{PkgRequest, PreReleasePolicy, RangeIdent, Request, RequestedBy};
use spk_schema::ident_build::Build;
use spk_schema::{Recipe, SpecRecipe};
use spk_solve::{BoxedResolverCallback, DefaultResolver, ResolverCallback, Solver};
use spk_storage::{self as storage};

use super::Tester;

pub struct PackageInstallTester<'a> {
    prefix: PathBuf,
    recipe: SpecRecipe,
    script: String,
    repos: Vec<Arc<storage::RepositoryHandle>>,
    options: OptionMap,
    additional_requirements: Vec<Request>,
    source: Option<PathBuf>,
    env_resolver: BoxedResolverCallback<'a>,
}

impl<'a> PackageInstallTester<'a> {
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

    /// Run the test script in the given working dir rather
    /// than inheriting the current one.
    pub fn with_source(&mut self, source: Option<PathBuf>) -> &mut Self {
        self.source = source;
        self
    }

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

    pub async fn test(&mut self) -> Result<()> {
        let mut rt = spfs::active_runtime().await?;
        rt.reset_all()?;
        rt.status.editable = true;
        rt.status.stack.clear();

        let mut solver = Solver::default();
        solver.set_binary_only(true);
        solver.update_options(self.options.clone());
        for repo in self.repos.iter().cloned() {
            solver.add_repository(repo);
        }

        // Request the specific build that goes with the selected build variant.
        let build_digest_for_variant = self.recipe.resolve_options(&self.options)?.digest();

        let build_to_test = self
            .recipe
            .ident()
            .to_any(None)
            .with_build(Some(Build::Digest(build_digest_for_variant)));

        let pkg = RangeIdent::double_equals(&build_to_test, [Component::All]);
        let request = PkgRequest::new(pkg, RequestedBy::InstallTest(self.recipe.ident().clone()))
            .with_prerelease(PreReleasePolicy::IncludeAll)
            .with_pin(None)
            .with_compat(None);
        solver.add_request(request.into());
        for request in self.additional_requirements.drain(..) {
            solver.add_request(request)
        }

        let (solution, _) = self.env_resolver.solve(&solver).await?;

        for layer in resolve_runtime_layers(&solution).await? {
            rt.push_digest(layer);
        }
        rt.save_state_to_storage().await?;
        spfs::remount_runtime(&rt).await?;

        let env = solution.to_environment(Some(std::env::vars()));

        let source_dir = match &self.source {
            Some(source) => source.clone(),
            None => PathBuf::from("."),
        };

        self.execute_test_script(&source_dir, env, &rt)
    }
}

#[async_trait::async_trait]
impl<'a> Tester for PackageInstallTester<'a> {
    async fn test(&mut self) -> Result<()> {
        PackageInstallTester::test(self).await
    }
    fn prefix(&self) -> &Path {
        &self.prefix
    }
    fn script(&self) -> &String {
        &self.script
    }
}
