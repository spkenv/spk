// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::str::FromStr;
use std::sync::Arc;

use clap::Args;
use miette::{Context, Result};
use spk_build::BuildSource;
use spk_cli_common::flags::VariantBuildStatus;
use spk_cli_common::{CommandArgs, Run, flags};
use spk_schema::foundation::format::FormatOptionMap;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::option_map::{HOST_OPTIONS, OptionMap};
use spk_schema::ident::RequestWithOptions;
use spk_schema::prelude::*;
use spk_schema::{Recipe, TestStage};

use crate::test::{PackageBuildTester, PackageInstallTester, PackageSourceTester, Tester};

#[cfg(test)]
#[path = "./cmd_test_test.rs"]
mod cmd_test_test;

/// Run package tests
///
/// In order to run install tests the package must have been built already
#[derive(Args)]
pub struct CmdTest {
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub workspace: flags::Workspace,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Test in the current directory, instead of the source package
    ///
    /// This is mostly relevant when testing source and build stages
    #[clap(long)]
    here: bool,

    /// The package(s) to test
    ///
    /// This can be a file name or `<name>/<version>` of an existing package
    /// from the repository. In either case, a stage can be specified to
    /// limit which tests are executed.
    #[clap(name = "FILE|PKG/VER[@STAGE]", required = true)]
    packages: Vec<String>,

    /// Test only the specified variants
    #[clap(flatten)]
    pub variant: flags::Variant,
}

#[async_trait::async_trait]
impl Run for CmdTest {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let options = self.options.get_options()?;
        let (_runtime, repos) = tokio::try_join!(
            self.runtime.ensure_active_runtime(&["test"]),
            self.solver.repos.get_repos_for_non_destructive_operation()
        )?;
        let repos = repos
            .into_iter()
            .map(|(_, r)| Arc::new(r))
            .collect::<Vec<_>>();
        let mut workspace = self.workspace.load_or_default()?;

        let source = if self.here { Some(".".into()) } else { None };

        let opt_host_options =
            (!self.options.no_host).then(|| HOST_OPTIONS.get().unwrap_or_default());

        // This includes any host options added by command line flag,
        // or not if --nohost was used.
        let options_reqs: Vec<RequestWithOptions> = self
            .options
            .get_var_requests()?
            .into_iter()
            .map(RequestWithOptions::Var)
            .collect();

        for package in &self.packages {
            let (name, stages) = match package.split_once('@') {
                Some((name, stage)) => {
                    let stage = TestStage::from_str(stage)?;
                    (name.to_string(), vec![stage])
                }
                None => {
                    let stages = vec![TestStage::Sources, TestStage::Build, TestStage::Install];
                    (package.to_string(), stages)
                }
            };

            let (spec_data, filename) = flags::find_package_recipe_from_workspace_or_repo(
                Some(&name),
                &options,
                &mut workspace,
                &repos,
            )
            .await?;
            let recipe = spec_data.into_recipe().wrap_err_with(|| {
                format!(
                    "{filename} was expected to contain a recipe",
                    filename = filename.to_string_lossy()
                )
            })?;

            for stage in stages {
                tracing::info!("Testing {}@{stage}...", filename.display());

                let default_variants = recipe.default_variants(&options);
                let variants_to_test = self
                    .variant
                    .requested_variants(
                        &recipe,
                        &default_variants,
                        &options,
                        opt_host_options.as_ref(),
                    )
                    .collect::<Result<Vec<_>>>()?;

                for variant_info in variants_to_test {
                    let VariantBuildStatus::Enabled(variant) = variant_info.build_status else {
                        continue;
                    };

                    let variant = {
                        let mut opts = match self.options.no_host {
                            true => OptionMap::default(),
                            false => HOST_OPTIONS.get()?,
                        };

                        opts.extend(variant.options().into_owned());
                        opts.extend(options.clone());

                        (*variant).clone().with_overrides(opts)
                    };

                    let selected = recipe
                        .get_tests(stage, &variant)
                        .wrap_err("Failed to select tests for this variant")?;
                    tracing::info!(
                        variant=%variant.options().format_option_map(),
                        "Running {} relevant tests for this variant",
                        selected.len()
                    );
                    for (index, test) in selected.into_iter().enumerate() {
                        let mut builder = self
                            .solver
                            .decision_formatter_settings
                            .get_formatter_builder(self.verbose)?;
                        let src_formatter = builder.with_header("Source Resolver ").build();
                        let build_src_formatter =
                            builder.with_header("Build Source Resolver ").build();
                        let build_formatter = builder.with_header("Build Resolver ").build();
                        let install_formatter =
                            builder.with_header("Install Env Resolver ").build();

                        let mut tester: Box<dyn Tester> = match stage {
                            TestStage::Sources => {
                                let solver = self.solver.get_solver(&self.options).await?;

                                let mut tester = PackageSourceTester::new(
                                    (*recipe).clone(),
                                    test.script(),
                                    solver,
                                );

                                tester
                                    .with_options(variant.options().into_owned())
                                    .with_repositories(repos.iter().cloned())
                                    .with_requirements(
                                        test.additional_requirements_with_options(
                                            &variant.options(),
                                        ),
                                    )
                                    .with_source(source.clone())
                                    .watch_environment_formatter(src_formatter);

                                Box::new(tester)
                            }

                            TestStage::Build => {
                                let solver = self.solver.get_solver(&self.options).await?;

                                let mut tester = PackageBuildTester::new(
                                    (*recipe).clone(),
                                    test.script(),
                                    solver,
                                );

                                tester
                                    .with_options(variant.options().into_owned())
                                    .with_repositories(repos.iter().cloned())
                                    .with_requirements(
                                        variant.additional_requirements().iter().cloned().chain(
                                            test.additional_requirements_with_options(
                                                &variant.options(),
                                            ),
                                        ),
                                    )
                                    .with_source(
                                        source.clone().map(BuildSource::LocalPath).unwrap_or_else(
                                            || {
                                                BuildSource::SourcePackage(
                                                    recipe
                                                        .ident()
                                                        .to_any_ident(Some(Build::Source))
                                                        .into(),
                                                )
                                            },
                                        ),
                                    )
                                    .with_source_formatter(build_src_formatter)
                                    .with_build_formatter(build_formatter);

                                Box::new(tester)
                            }

                            TestStage::Install => {
                                let solver = self.solver.get_solver(&self.options).await?;

                                let mut tester = PackageInstallTester::new(
                                    (*recipe).clone(),
                                    test.script(),
                                    &variant,
                                    solver,
                                );

                                tester
                                    .with_options(variant.options().into_owned())
                                    .with_repositories(repos.iter().cloned())
                                    .with_requirements(
                                        test.additional_requirements_with_options(
                                            &variant.options(),
                                        ),
                                    )
                                    .with_requirements(options_reqs.clone())
                                    .with_source(source.clone())
                                    .watch_environment_formatter(install_formatter);

                                Box::new(tester)
                            }
                        };

                        tracing::info!(
                            variant=%variant.options().format_option_map(),
                            "Running selected test #{index}",
                        );

                        tester.test().await?
                    }
                }
            }
        }
        Ok(0)
    }
}

impl CommandArgs for CmdTest {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for a test are the packages
        self.packages.clone()
    }
}
