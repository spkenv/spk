// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::io::Write;

use anyhow::Result;
use clap::Args;
use colored::Colorize;
use itertools::Itertools;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::ident::parse_ident;
use spk_schema::{BuildIdent, VersionIdent};
use spk_storage as storage;

/// Remove a package from a repository
#[derive(Args)]
#[clap(visible_alias = "rm")]
pub struct Remove {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    /// Do not ask for confirmations (dangerous!)
    #[clap(short, long)]
    yes: bool,

    #[clap(name = "PKG", required = true)]
    packages: Vec<String>,
}

#[async_trait::async_trait]
impl Run for Remove {
    async fn run(&mut self) -> Result<i32> {
        let repos = self.repos.get_repos_for_destructive_operation().await?;
        if repos.is_empty() {
            eprintln!(
                "{}",
                "No repositories selected, specify --enable-repo (-r)".yellow()
            );
            return Ok(1);
        }

        for name in &self.packages {
            if !name.contains('/') && !self.yes {
                let mut input = String::new();
                print!(
                    "{}",
                    format!(
                        "Are you sure that you want to remove all versions of {name} from {repos}? [y/N]: ",
                        repos = repos.iter().map(|(name, _)| name).join(", ")
                    )
                    .yellow()
                );
                let _ = std::io::stdout().flush();
                std::io::stdin().read_line(&mut input)?;
                match input.trim() {
                    "y" | "yes" => {}
                    _ => {
                        println!("Removal cancelled");
                        return Ok(1);
                    }
                }
            }

            for (repo_name, repo) in repos.iter() {
                let pkg = parse_ident(name)?;
                let versions = if name.contains('/') {
                    vec![pkg]
                } else {
                    repo.list_package_versions(pkg.name())
                        .await?
                        .iter()
                        .map(|v| pkg.with_version((**v).clone()))
                        .collect()
                };

                for version in versions {
                    match version.into_inner() {
                        (version, None) => {
                            remove_all(repo_name, repo, &version).await?;
                        }
                        (version, Some(build)) => {
                            remove_build(repo_name, repo, &version.into_build(build)).await?;
                        }
                    }
                }
            }
        }
        Ok(0)
    }
}

impl CommandArgs for Remove {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for a remove are the packages
        self.packages.clone()
    }
}

async fn remove_build(
    repo_name: &str,
    repo: &storage::RepositoryHandle,
    pkg: &BuildIdent,
) -> Result<()> {
    let repo_name = repo_name.bold();
    let pretty_pkg = pkg.format_ident();
    let (recipe, package) = tokio::join!(repo.remove_recipe(pkg), repo.remove_package(pkg),);
    // First inform on the things that actually happened.
    if recipe.is_ok() {
        tracing::info!("removed recipe {pretty_pkg: >25} from {repo_name}")
    }
    if package.is_ok() {
        tracing::info!("removed build  {pretty_pkg: >25} from {repo_name}")
    }
    // Treat "not found" problems as warnings unless both parts were not
    // found.
    let recipe_not_found = matches!(
        recipe,
        Err(spk_storage::Error::SpkValidatorsError(
            spk_schema::validators::Error::PackageNotFoundError(_)
        ))
    );
    let pkg_not_found = matches!(
        package,
        Err(spk_storage::Error::SpkValidatorsError(
            spk_schema::validators::Error::PackageNotFoundError(_)
        ))
    );
    if recipe_not_found && pkg_not_found {
        // Both parts were not found; this is a hard error so don't
        // emit warnings.
        return Err(package.unwrap_err().into());
    }
    // When not erroring above, emit a warning about the one that was missing.
    if recipe_not_found {
        tracing::warn!("recipe {pretty_pkg: >25} not found in {repo_name}");
    } else if pkg_not_found {
        tracing::warn!("build  {pretty_pkg: >25} not found in {repo_name}");
    }
    // Now fail if anything errored for other reasons.
    if package.is_err() && !pkg_not_found {
        return Err(package.unwrap_err().into());
    }
    if recipe.is_err() && !recipe_not_found {
        return Err(recipe.unwrap_err().into());
    }
    Ok(())
}

async fn remove_all(
    repo_name: &str,
    repo: &storage::RepositoryHandle,
    pkg: &VersionIdent,
) -> Result<()> {
    let pretty_pkg = pkg.format_ident();
    for build in repo.list_package_builds(pkg).await? {
        remove_build(repo_name, repo, &build).await?
    }
    let repo_name = repo_name.bold();
    match repo.remove_recipe(pkg).await {
        Ok(()) => tracing::info!("removed recipe {pretty_pkg: >25} from {repo_name}"),
        Err(spk_storage::Error::SpkValidatorsError(
            spk_schema::validators::Error::PackageNotFoundError(_),
        )) => {
            tracing::warn!("spec {pretty_pkg: >25} not found in {repo_name}")
        }
        Err(err) => return Err(err.into()),
    }
    Ok(())
}
