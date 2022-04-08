// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::io::Write;

use anyhow::Result;
use clap::Args;
use colored::Colorize;
use spk::api;

use super::flags;

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

impl Remove {
    pub fn run(&self) -> Result<i32> {
        let repos = self.repos.get_repos(None)?;
        if repos.is_empty() {
            eprintln!(
                "{}",
                "No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r)"
                    .yellow()
            );
            return Ok(1);
        }

        for name in &self.packages {
            if !name.contains('/') && !self.yes {
                let mut input = String::new();
                print!(
                    "{}",
                    format!("Are you sure that you want to remove all versions of {name}? [y/N]: ")
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
                let pkg = api::parse_ident(&name)?;
                let versions = if name.contains('/') {
                    vec![pkg]
                } else {
                    repo.list_package_versions(&name)?
                        .into_iter()
                        .map(|v| pkg.with_version(v))
                        .collect()
                };

                for version in versions {
                    if version.build.is_some() {
                        remove_build(&repo_name, &repo, &version)?;
                    } else {
                        remove_all(&repo_name, &repo, &version)?;
                    }
                }
            }
        }
        Ok(0)
    }
}

fn remove_build(
    repo_name: &String,
    repo: &spk::storage::RepositoryHandle,
    pkg: &spk::api::Ident,
) -> Result<()> {
    let repo_name = repo_name.bold();
    let pretty_pkg = spk::io::format_ident(&pkg);
    match repo.remove_spec(pkg) {
        Ok(()) => {
            tracing::info!("removed build spec {pretty_pkg} from {repo_name}")
        }
        Err(spk::Error::PackageNotFoundError(_)) => {
            tracing::warn!("spec {pretty_pkg} not found in {repo_name}")
        }
        Err(err) => return Err(err.into()),
    }
    match repo.remove_package(pkg) {
        Ok(()) => {
            tracing::info!("removed build      {pretty_pkg} from {repo_name}")
        }
        Err(spk::Error::PackageNotFoundError(_)) => {
            tracing::warn!("build {pretty_pkg} not found in {repo_name}")
        }
        Err(err) => return Err(err.into()),
    }
    Ok(())
}

fn remove_all(
    repo_name: &String,
    repo: &spk::storage::RepositoryHandle,
    pkg: &spk::api::Ident,
) -> Result<()> {
    let pretty_pkg = spk::io::format_ident(pkg);
    for build in repo.list_package_builds(pkg)? {
        remove_build(repo_name, repo, &build)?
    }
    let repo_name = repo_name.bold();
    match repo.remove_spec(pkg) {
        Ok(()) => tracing::info!("removed spec       {pretty_pkg} from {repo_name}"),
        Err(spk::Error::PackageNotFoundError(_)) => {
            tracing::warn!("spec {pretty_pkg} not found in {repo_name}")
        }
        Err(err) => return Err(err.into()),
    }
    Ok(())
}
