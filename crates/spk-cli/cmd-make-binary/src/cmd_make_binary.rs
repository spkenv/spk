// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use clap::Args;
use futures::TryFutureExt;
use itertools::Itertools;
use miette::{Context, IntoDiagnostic, Report, Result, bail, miette};
use spk_build::{BinaryPackageBuilder, BuildSource};
use spk_cli_common::{BuildArtifact, BuildResult, CommandArgs, Run, flags, spk_exe};
use spk_schema::OptionMap;
use spk_schema::foundation::format::FormatIdent;
use spk_schema::ident::{InitialRawRequest, PkgRequest, RequestedBy};
use spk_schema::option_map::HOST_OPTIONS;
use spk_schema::prelude::*;
use spk_storage as storage;

#[cfg(test)]
#[path = "./cmd_make_binary_test.rs"]
mod cmd_make_binary_test;

/// Build a binary package from a spec file or source package.
#[derive(Args)]
#[clap(visible_aliases = &["mkbinary", "mkbin", "mkb"])]
pub struct MakeBinary {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Build from the current directory, instead of a source package)
    #[clap(long)]
    pub here: bool,

    /// Setup the build, but instead of running the build script start an interactive shell
    #[clap(long, short)]
    pub interactive: bool,

    /// Build the first variant of this package, and then immediately enter a shell environment with it
    #[clap(long, short)]
    pub env: bool,

    #[clap(flatten)]
    pub packages: flags::Packages,

    /// Build only the specified variants
    #[clap(flatten)]
    pub variant: flags::Variant,

    /// Allow dependencies of the package being built to have a dependency on
    /// this package.
    #[clap(long)]
    pub allow_circular_dependencies: bool,

    /// Populated with created specs to generate a summary from the caller.
    #[clap(skip)]
    pub created_builds: BuildResult,
}

impl CommandArgs for MakeBinary {
    // The important positional args for a make-binary are the packages
    fn get_positional_args(&self) -> Vec<String> {
        self.packages.get_positional_args()
    }
}

#[async_trait::async_trait]
impl Run for MakeBinary {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        if spfs::get_config()?
            .storage
            .allow_payload_sharing_between_users
        {
            bail!(
                "Building packages disabled when 'allow_payload_sharing_between_users' is enabled"
            );
        }

        let options = self.options.get_options()?;
        #[rustfmt::skip]
        let (_runtime, local, repos) = tokio::try_join!(
            self.runtime.ensure_active_runtime(&["make-binary", "mkbinary", "mkbin", "mkb"]),
            storage::local_repository().map_ok(storage::RepositoryHandle::from).map_err(miette::Error::from),
            async { self.solver.repos.get_repos_for_non_destructive_operation().await }
        )?;
        let repos = repos
            .into_iter()
            .map(|(_, r)| Arc::new(r))
            .collect::<Vec<_>>();

        let opt_host_options =
            (!self.options.no_host).then(|| HOST_OPTIONS.get().unwrap_or_default());

        for (package, spec_data, filename) in
            self.packages.find_all_recipes(&options, &repos).await?
        {
            let recipe = spec_data.into_recipe().wrap_err_with(|| {
                format!(
                    "{filename} was expected to contain a recipe",
                    filename = filename.to_string_lossy()
                )
            })?;
            let ident = recipe.ident();

            tracing::info!("saving package recipe for {}", ident.format_ident());
            local.force_publish_recipe(&recipe).await?;

            tracing::info!("building binary package(s) for {}", ident.format_ident());

            let default_variants = recipe.default_variants(&options);
            let variants_to_build = self
                .variant
                .requested_variants(
                    &recipe,
                    &default_variants,
                    &options,
                    opt_host_options.as_ref(),
                )
                .collect::<Result<Vec<_>>>()?;

            for variant_info in &variants_to_build {
                let variant = match &variant_info.build_status {
                    flags::VariantBuildStatus::Enabled(variant) => variant,
                    flags::VariantBuildStatus::FilteredOut(mismatches) => {
                        tracing::debug!(
                            "Skipping variant that was filtered out:\n{this_location} didn't match on {mismatches}",
                            this_location = variant_info.location,
                            mismatches = mismatches.keys().join(", ")
                        );
                        continue;
                    }
                    flags::VariantBuildStatus::Duplicate(location) => {
                        tracing::debug!(
                            "Skipping variant that was already built:\n{this_location} is a duplicate of {location}",
                            this_location = variant_info.location
                        );
                        continue;
                    }
                };

                let mut overrides = OptionMap::default();
                if !self.options.no_host {
                    overrides.extend(HOST_OPTIONS.get()?);
                }
                overrides.extend(options.clone());
                let variant = (**variant).clone().with_overrides(overrides);

                tracing::info!(
                    "building {location}:\n{variant}",
                    location = variant_info.location
                );

                // Always show the solution packages for the solves
                let mut fmt_builder = self
                    .solver
                    .decision_formatter_settings
                    .get_formatter_builder(self.verbose)?;
                let src_formatter = fmt_builder
                    .with_solution(true)
                    .with_header("Src Resolver ")
                    .build();
                let build_formatter = fmt_builder
                    .with_solution(true)
                    .with_header("Build Resolver ")
                    .build();

                let solver = self.solver.get_solver(&self.options).await?;
                let mut builder =
                    BinaryPackageBuilder::from_recipe_with_solver((*recipe).clone(), solver);
                builder
                    .with_repositories(repos.iter().cloned())
                    .set_interactive(self.interactive)
                    .with_source_formatter(src_formatter)
                    .with_build_formatter(build_formatter)
                    .with_allow_circular_dependencies(self.allow_circular_dependencies);

                if self.here {
                    let here = std::env::current_dir()
                        .into_diagnostic()
                        .wrap_err("Failed to get current directory")?;
                    builder.with_source(BuildSource::LocalPath(here));
                } else if let Some(flags::PackageSpecifier::WithSourceIdent((_, ref ident))) =
                    package
                {
                    // Use the source package `AnyIdent` if the caller supplied one.
                    builder.with_source(BuildSource::SourcePackage(ident.clone()));
                }
                let out = match builder.build_and_publish(&variant, &local).await {
                    Err(err @ spk_build::Error::SpkSolverError(_))
                    | Err(
                        err @ spk_build::Error::SpkStorageError(spk_storage::Error::VersionExists(
                            _,
                        )),
                    )
                    | Err(
                        err @ spk_build::Error::SpkStorageError(
                            spk_storage::Error::PackageNotFound(_),
                        ),
                    ) => {
                        if !self.created_builds.is_empty() {
                            tracing::warn!("Completed builds:");
                            for (_, artifact) in self.created_builds.iter() {
                                tracing::warn!("   {artifact}");
                            }
                        }

                        tracing::error!(
                            "{location} failed:\n{variant}",
                            location = variant_info.location
                        );
                        return Err(err.into());
                    }
                    Ok((spec, _cmpts)) => spec,
                    Err(err) => return Err(err.into()),
                };
                tracing::info!("created {}", out.ident().format_ident());
                self.created_builds.push(
                    filename.to_string_lossy().to_string(),
                    BuildArtifact::Binary(
                        out.ident().clone(),
                        variant_info.location,
                        variant.options().into_owned(),
                    ),
                );

                if self.env {
                    let ident = out.ident().to_any_ident();
                    let request = PkgRequest::from_ident(
                        ident.clone(),
                        RequestedBy::CommandLineRequest(InitialRawRequest(ident.to_string())),
                    );
                    let mut cmd = std::process::Command::new(spk_exe());
                    cmd.args(["env", "--enable-repo", "local"])
                        .arg(request.pkg.to_string());
                    tracing::info!("entering environment with new package...");
                    tracing::debug!("{:?}", cmd);
                    let status = cmd.status().into_diagnostic()?;
                    return Ok(status.code().unwrap_or(1));
                }
            }

            // If nothing was built (i.e., variant filters didn't match anything),
            // treat this as an error.
            if self.created_builds.is_empty() {
                let help = "Check --variant filters or host options match at least one variant";
                let mut report: Option<Report> = None;
                for variant_info in variants_to_build.iter().rev() {
                    if let flags::VariantBuildStatus::FilteredOut(mismatches) =
                        &variant_info.build_status
                    {
                        let message = format!(
                            "{location} didn't match on {mismatches}",
                            location = variant_info.location,
                            mismatches = mismatches
                                .iter()
                                .map(|(k, v)| {
                                    if let Some(actual) = &v.actual {
                                        format!(
                                            "{k} (expected {expected}, variant has {actual})",
                                            expected = v.expected
                                        )
                                    } else {
                                        format!("{k} (missing from variant)")
                                    }
                                })
                                .join(", ")
                        );

                        match report {
                            Some(r) => report = Some(r.wrap_err(message)),
                            None => {
                                report = Some(miette!(help = help, "{message}"));
                            }
                        }
                    }
                }
                return Err(match report {
                    Some(report) => report.wrap_err("No packages were built"),
                    None => {
                        miette!(help = help, "No packages were built")
                    }
                });
            }
        }

        Ok(0)
    }
}
