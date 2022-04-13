// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::BTreeSet;

use anyhow::Result;
use clap::Args;
use colored::Colorize;

use super::flags;

/// List packages in one or more repositories
#[derive(Args)]
#[clap(visible_alias = "list")]
pub struct Ls {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// Show available package components in the output
    #[clap(long, short)]
    components: bool,

    /// Recursively list all package versions and builds (recursive results are not sorted)
    #[clap(long)]
    recursive: bool,

    /// Given a name, list versions. Given a name/version list builds.
    ///
    /// If nothing is provided, list all available packages.
    #[clap(name = "NAME[/VERSION]")]
    package: Option<String>,
}

impl Ls {
    pub fn run(&mut self) -> Result<i32> {
        let mut repos = self.repos.get_repos(None)?;

        if repos.is_empty() {
            let local = String::from("local");
            if !self.repos.disable_repo.contains(&local) {
                self.repos.local_repo = true;
                repos = self.repos.get_repos(None)?;
            } else {
                eprintln!(
                    "{}",
                    "No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r)"
                        .yellow()
                );
                return Ok(1);
            }
        }

        if self.recursive {
            return self.list_recursively(repos);
        }

        let mut results = BTreeSet::new();
        match &self.package {
            None => {
                for (_repo_name, repo) in repos {
                    results.extend(repo.list_packages()?)
                }
            }
            Some(package) if !package.contains('/') => {
                for (_, repo) in repos {
                    results.extend(
                        repo.list_package_versions(package)?
                            .iter()
                            .map(ToString::to_string),
                    );
                }
            }
            Some(package) => {
                let pkg = spk::api::parse_ident(package)?;
                for (_, repo) in repos {
                    for build in repo.list_package_builds(&pkg)? {
                        results.insert(self.format_build(&build, &repo)?);
                    }
                }
            }
        }

        for item in results {
            println!("{}", item);
        }
        Ok(0)
    }

    fn list_recursively(
        &self,
        repos: Vec<(String, spk::storage::RepositoryHandle)>,
    ) -> Result<i32> {
        for (_repo_name, repo) in repos {
            let packages = match &self.package {
                Some(package) => vec![package.to_owned()],
                None => repo.list_packages()?,
            };
            for package in packages {
                let versions = if package.contains('/') {
                    vec![spk::api::parse_ident(&package)?]
                } else {
                    let base = spk::api::Ident::new(&package)?;
                    repo.list_package_versions(&package)?
                        .into_iter()
                        .map(|v| base.with_version(v))
                        .collect()
                };
                for pkg in versions {
                    for build in repo.list_package_builds(&pkg)? {
                        println!("{}", self.format_build(&build, &repo)?);
                    }
                }
            }
        }
        Ok(0)
    }

    fn format_build(
        &self,
        pkg: &spk::api::Ident,
        repo: &spk::storage::RepositoryHandle,
    ) -> Result<String> {
        if pkg.build.is_none() || pkg.is_source() {
            return Ok(spk::io::format_ident(pkg));
        }

        let mut item = spk::io::format_ident(pkg);
        if self.verbose > 0 {
            let spec = repo.read_spec(pkg)?;
            let options = spec.resolve_all_options(&spk::api::OptionMap::default());
            item.push(' ');
            item.push_str(&spk::io::format_options(&options));
        }
        if self.verbose > 1 || self.components {
            let cmpts = repo.get_package(pkg)?;
            item.push(' ');
            item.push_str(&spk::io::format_components(cmpts.keys()));
        }
        Ok(item)
    }
}
