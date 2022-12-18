// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;
use futures::TryFutureExt;
use spk_build::{BinaryPackageBuilder, BuildSource};
use spk_cli_common::{flags, spk_exe, CommandArgs, Run};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::ident::{PkgRequest, RangeIdent, RequestedBy};
use spk_schema::prelude::*;
use spk_storage as storage;

#[derive(Clone, Debug)]
pub enum PackageSpecifier {
    Plain(String),
    WithSourceIdent((String, RangeIdent)),
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

    /// The local yaml spec files or published package/versions to build or rebuild
    #[clap(name = "SPEC_FILE|PKG/VER")]
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
            self.runtime.ensure_active_runtime(&["make-binary", "mkbinary", "mkbin", "mkb"]),
            storage::local_repository().map_ok(storage::RepositoryHandle::from).map_err(anyhow::Error::from),
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
            let (recipe, _) = flags::find_package_recipe_from_template_or_repo(
                &package.as_ref().map(|p| p.get_specifier().to_owned()),
                &options,
                &repos,
            )
            .await?;
            let ident = recipe.ident();

            tracing::info!("saving package recipe for {}", ident.format_ident());
            local.force_publish_recipe(&recipe).await?;

            tracing::info!("building binary package(s) for {}", ident.format_ident());
            let mut built = std::collections::HashSet::new();

            let default_variants = recipe.default_variants();
            let variants_to_build = match self.variant {
                Some(index) if index < default_variants.len() => {
                    default_variants.iter().skip(index).take(1)
                }
                Some(index) => {
                    anyhow::bail!(
                        "--variant {index} is out of range; {} variant(s) found in {}",
                        default_variants.len(),
                        recipe.ident().format_ident(),
                    );
                }
                None => default_variants.iter().skip(0).take(usize::MAX),
            };

            for variant in variants_to_build {
                let variant = spk_schema::ExtensionVariant::from(variant)
                    .with_host_options(!self.options.no_host)?
                    .with_overrides(options.clone());

                if !built.insert(variant.clone()) {
                    tracing::debug!("Skipping variant that was already built:\n{variant}");
                    continue;
                }

                tracing::info!("building variant:\n{variant}");

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

                let mut builder = BinaryPackageBuilder::from_recipe((*recipe).clone());
                builder
                    .with_repositories(repos.iter().cloned())
                    .set_interactive(self.interactive)
                    .with_source_resolver(&src_formatter)
                    .with_build_resolver(&build_formatter);

                if self.here {
                    let here =
                        std::env::current_dir().context("Failed to get current directory")?;
                    builder.with_source(BuildSource::LocalPath(here));
                } else if let Some(PackageSpecifier::WithSourceIdent((_, ref ident))) = package {
                    // Use the source package `AnyIdent` if the caller supplied one.
                    builder.with_source(BuildSource::SourcePackage(ident.clone()));
                }
                let out = match builder.build_and_publish(&variant, &local).await {
                    Err(err @ spk_build::Error::SpkSolverError(_))
                    | Err(
                        err @ spk_build::Error::SpkStorageError(
                            spk_storage::Error::SpkValidatorsError(
                                spk_schema::validators::Error::PackageNotFoundError(_),
                            ),
                        ),
                    ) => {
                        tracing::error!("variant failed:\n{variant}");
                        return Err(err.into());
                    }
                    Ok((spec, _cmpts)) => spec,
                    Err(err) => return Err(err.into()),
                };
                tracing::info!("created {}", out.ident().format_ident());

                if self.env {
                    let request =
                        PkgRequest::from_ident(out.ident().to_any(), RequestedBy::CommandLine);
                    let mut cmd = std::process::Command::new(spk_exe());
                    cmd.args(["env", "--enable-repo", "local"])
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
