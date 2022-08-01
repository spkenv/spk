// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;
use futures::TryFutureExt;
use spk::io::Format;
use spk::prelude::*;

use super::{flags, CommandArgs, Run};

#[derive(Clone)]
pub enum PackageSpecifier {
    Plain(String),
    WithSourceIdent((String, spk::api::RangeIdent)),
}

impl PackageSpecifier {
    // Return the package spec or filename string.
    fn get_specifier(&self) -> &String {
        match self {
            PackageSpecifier::Plain(s) => s,
            PackageSpecifier::WithSourceIdent((s, _)) => s,
        }
    }
}

impl std::str::FromStr for PackageSpecifier {
    type Err = clap::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // On the command line, only `Plain` is possible.
        Ok(PackageSpecifier::Plain(s.to_owned()))
    }
}

/// Build a binary package from a spec file or source package.
#[derive(Args)]
#[clap(visible_aliases = &["mkbinary", "mkbin", "mkb"])]
pub struct MakeBinary {
    #[clap(flatten)]
    pub repos: flags::Repositories,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// Build from the current directory, instead of a source package)
    #[clap(long)]
    pub here: bool,

    /// Setup the build, but instead of running the build script start an interactive shell
    #[clap(long, short)]
    pub interactive: bool,

    /// Build the first variant of this package, and then immediately enter a shell environment with it
    #[clap(long, short)]
    pub env: bool,

    /// The packages or yaml spec files to build
    #[clap(name = "PKG|SPEC_FILE")]
    pub packages: Vec<PackageSpecifier>,

    /// Build only the specified variant, by index, if defined
    #[clap(long)]
    pub variant: Option<usize>,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,
}

impl CommandArgs for MakeBinary {
    // The important positional args for a make-binary are the packages
    fn get_positional_args(&self) -> Vec<String> {
        self.packages
            .iter()
            .map(|ps| ps.get_specifier())
            .cloned()
            .collect()
    }
}

#[async_trait::async_trait]
impl Run for MakeBinary {
    async fn run(&mut self) -> Result<i32> {
        let options = self.options.get_options()?;
        #[rustfmt::skip]
        let (_runtime, local, repos) = tokio::try_join!(
            self.runtime.ensure_active_runtime(),
            spk::storage::local_repository().map_ok(spk::storage::RepositoryHandle::from).map_err(anyhow::Error::from),
            async { self.repos.get_repos_for_non_destructive_operation().await }
        )?;
        let repos = repos
            .into_iter()
            .map(|(_, r)| Arc::new(r))
            .collect::<Vec<_>>();

        let mut packages: Vec<_> = self.packages.iter().cloned().map(Some).collect();
        if packages.is_empty() {
            packages.push(None)
        }

        for package in packages {
            let template = match flags::find_package_template(
                &package.as_ref().map(|p| p.get_specifier().to_owned()),
            )? {
                flags::FindPackageTemplateResult::NotFound(name) => {
                    Arc::new(spk::api::SpecTemplate::from_file(name.as_ref())?)
                }
                res => {
                    let (_, template) = res.must_be_found();
                    template
                }
            };

            tracing::info!("rendering template for {}", template.name());
            let recipe = template.render(&options)?;
            let ident = recipe.ident();

            tracing::info!("saving package recipe for {}", ident.format_ident());
            local.force_publish_recipe(&recipe).await?;

            tracing::info!("building binary package(s) for {}", ident.format_ident());
            let mut built = std::collections::HashSet::new();

            let variants_to_build = match self.variant {
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

            for variant in variants_to_build {
                let mut opts = if !self.options.no_host {
                    spk::api::host_options()?
                } else {
                    spk::api::OptionMap::default()
                };

                opts.extend(variant.clone());
                opts.extend(options.clone());
                let digest = opts.digest_str();
                if !built.insert(digest) {
                    continue;
                }

                tracing::info!("building variant {}", spk::io::format_options(&opts));

                // Always show the solution packages for the solves
                let mut fmt_builder = self.formatter_settings.get_formatter_builder(self.verbose);
                let src_formatter = fmt_builder
                    .with_solution(true)
                    .with_header("Src Resolver ")
                    .build();
                let build_formatter = fmt_builder
                    .with_solution(true)
                    .with_header("Build Resolver ")
                    .build();

                let mut builder = spk::build::BinaryPackageBuilder::from_recipe(recipe.clone());
                builder
                    .with_options(opts.clone())
                    .with_repositories(repos.iter().cloned())
                    .set_interactive(self.interactive)
                    .with_source_resolver(&src_formatter)
                    .with_build_resolver(&build_formatter);

                if self.here {
                    let here =
                        std::env::current_dir().context("Failed to get current directory")?;
                    builder.with_source(spk::build::BuildSource::LocalPath(here));
                } else if let Some(PackageSpecifier::WithSourceIdent((_, ref ident))) = package {
                    // Use the source package `Ident` if the caller supplied one.
                    builder.with_source(spk::build::BuildSource::SourcePackage(ident.clone()));
                }
                let out = match builder.build_and_publish(&local).await {
                    Err(err @ spk::Error::Solve(_))
                    | Err(err @ spk::Error::PackageNotFoundError(_)) => {
                        tracing::error!("variant failed {}", spk::io::format_options(&opts));
                        return Err(err.into());
                    }
                    Ok((spec, _cmpts)) => spec,
                    Err(err) => return Err(err.into()),
                };
                tracing::info!("created {}", out.ident().format_ident());

                if self.env {
                    let request = spk::api::PkgRequest::from_ident(
                        out.ident().clone(),
                        spk::api::RequestedBy::CommandLine,
                    );
                    let mut cmd = std::process::Command::new(crate::env::spk_exe());
                    cmd.args(&["env", "--enable-repo", "local"])
                        .arg(request.pkg.to_string());
                    tracing::info!("entering environment with new package...");
                    tracing::debug!("{:?}", cmd);
                    let status = cmd.status()?;
                    return Ok(status.code().unwrap_or(1));
                }
            }
        }
        Ok(0)
    }
}
