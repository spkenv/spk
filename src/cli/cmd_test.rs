// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use clap::Args;
use spk::io::Format;
use spk::prelude::*;

use super::{flags, CommandArgs, Run};

#[cfg(test)]
#[path = "./cmd_test_test.rs"]
mod cmd_test_test;

/// Run package tests
///
/// In order to run install tests the package must have been built already
#[derive(Args)]
pub struct Test {
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,
    #[clap(flatten)]
    pub repos: flags::Repositories,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// Test in the current directory, instead of the source package
    ///
    /// This is mostly relevant when testing source and build stages
    #[clap(long)]
    here: bool,

    /// The package(s) to test
    ///
    /// This can be a file name or <name>/<version> of an existing package
    /// from the repository. In either case, a stage can be specified to
    /// limit which tests are executed.
    #[clap(name = "FILE|PKG[@STAGE]", required = true)]
    packages: Vec<String>,

    /// Test only the specified variant, by index, if defined
    #[clap(long, hide = true)]
    pub variant: Option<usize>,
}

#[async_trait::async_trait]
impl Run for Test {
    async fn run(&mut self) -> Result<i32> {
        let options = self.options.get_options()?;
        let (_runtime, repos) = tokio::try_join!(
            self.runtime.ensure_active_runtime(),
            self.repos.get_repos_for_non_destructive_operation()
        )?;
        let repos = repos
            .into_iter()
            .map(|(_, r)| Arc::new(r))
            .collect::<Vec<_>>();

        let source = if self.here { Some(".".into()) } else { None };

        for package in &self.packages {
            let (name, stages) = match package.split_once('@') {
                Some((name, stage)) => {
                    let stage = spk::api::TestStage::from_str(stage)?;
                    (name.to_string(), vec![stage])
                }
                None => {
                    let stages = vec![
                        spk::api::TestStage::Sources,
                        spk::api::TestStage::Build,
                        spk::api::TestStage::Install,
                    ];
                    (package.to_string(), stages)
                }
            };

            let (recipe, filename) = match flags::find_package_template(&Some(name.clone()))? {
                flags::FindPackageTemplateResult::Found { path, template } => {
                    let recipe = template.render(&options)?;
                    (Arc::new(recipe), path)
                }
                _ => {
                    let pkg = spk::api::parse_ident(&name)?;
                    let mut found = None;
                    for repo in repos.iter() {
                        match repo.read_recipe(&pkg).await {
                            Ok(recipe) => {
                                found = Some((recipe, std::path::PathBuf::from(&name)));
                                break;
                            }
                            Err(spk::Error::PackageNotFoundError(_)) => continue,
                            Err(err) => return Err(err.into()),
                        }
                    }
                    found.ok_or(spk::Error::PackageNotFoundError(pkg))?
                }
            };

            for stage in stages {
                tracing::info!("Testing {}@{stage}...", filename.display());

                let mut tested = std::collections::HashSet::new();

                let variants_to_test = match self.variant {
                    Some(index) if index < recipe.default_variants().len() => {
                        recipe.default_variants().iter().skip(index).take(1)
                    }
                    Some(index) => {
                        anyhow::bail!(
                            "--variant {index} is out of range; {} variant(s) found in {}",
                            recipe.default_variants().len(),
                            recipe.ident().format_ident(),
                        );
                    }
                    None => recipe.default_variants().iter().skip(0).take(usize::MAX),
                };

                for variant in variants_to_test {
                    let mut opts = match self.options.no_host {
                        true => spk::api::OptionMap::default(),
                        false => spk::api::host_options()?,
                    };

                    opts.extend(variant.clone());
                    opts.extend(options.clone());
                    let digest = opts.digest();
                    if !tested.insert(digest) {
                        continue;
                    }

                    for (index, test) in recipe.get_tests(&opts)?.into_iter().enumerate() {
                        if test.stage != stage {
                            continue;
                        }

                        let mut selected = false;
                        for selector in test.selectors.iter() {
                            let mut selected_opts = opts.clone();
                            selected_opts.extend(selector.clone());
                            if selected_opts.digest() == digest {
                                selected = true;
                            }
                        }
                        if !selected && !test.selectors.is_empty() {
                            tracing::info!(
                                "SKIP #{index}: variant not selected: {}",
                                spk::io::format_options(&opts)
                            );
                            continue;
                        }
                        tracing::info!(
                            "Running test #{index} variant={}",
                            spk::io::format_options(&opts)
                        );

                        let mut builder =
                            self.formatter_settings.get_formatter_builder(self.verbose);
                        let src_formatter = builder.with_header("Source Resolver ").build();
                        let build_src_formatter =
                            builder.with_header("Build Source Resolver ").build();
                        let build_formatter = builder.with_header("Build Resolver ").build();
                        let install_formatter =
                            builder.with_header("Install Env Resolver ").build();

                        match stage {
                            spk::api::TestStage::Sources => {
                                spk::test::PackageSourceTester::new(
                                    (*recipe).clone(),
                                    test.script.join("\n"),
                                )
                                .with_options(opts.clone())
                                .with_repositories(repos.iter().cloned())
                                .with_requirements(test.requirements.clone())
                                .with_source(source.clone())
                                .watch_environment_resolve(&src_formatter)
                                .test()
                                .await?
                            }

                            spk::api::TestStage::Build => {
                                spk::test::PackageBuildTester::new(
                                    (*recipe).clone(),
                                    test.script.join("\n"),
                                )
                                .with_options(opts.clone())
                                .with_repositories(repos.iter().cloned())
                                .with_requirements(test.requirements.clone())
                                .with_source(
                                    source
                                        .clone()
                                        .map(spk::build::BuildSource::LocalPath)
                                        .unwrap_or_else(|| {
                                            spk::build::BuildSource::SourcePackage(
                                                recipe
                                                    .ident()
                                                    .with_build(spk::api::Build::Source)
                                                    .into(),
                                            )
                                        }),
                                )
                                .with_source_resolver(&build_src_formatter)
                                .with_build_resolver(&build_formatter)
                                .test()
                                .await?
                            }

                            spk::api::TestStage::Install => {
                                spk::test::PackageInstallTester::new(
                                    (*recipe).clone(),
                                    test.script.join("\n"),
                                )
                                .with_options(opts.clone())
                                .with_repositories(repos.iter().cloned())
                                .with_requirements(test.requirements.clone())
                                .with_source(source.clone())
                                .watch_environment_resolve(&install_formatter)
                                .test()
                                .await?
                            }
                        }
                    }
                }
            }
        }
        Ok(0)
    }
}

impl CommandArgs for Test {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for a test are the packages
        self.packages.clone()
    }
}
