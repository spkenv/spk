// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::BTreeSet;

use anyhow::Result;
use clap::Args;
use colored::Colorize;

use super::{flags, Run};

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

    /// Recursively list all package versions and builds
    #[clap(long)]
    recursive: bool,

    /// Given a name, list versions. Given a name/version list builds.
    ///
    /// If nothing is provided, list all available packages.
    #[clap(name = "NAME[/VERSION]")]
    package: Option<String>,
}

impl Run for Ls {
    fn run(&mut self) -> Result<i32> {
        let mut repos = self.repos.get_repos(None)?;

        if repos.is_empty() {
            let local = String::from("local");
            if !self.repos.disable_repo.contains(&local) {
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
                    results.extend(repo.list_packages()?.into_iter().map(spk::api::Name::into))
                }
            }
            Some(package) if !package.contains('/') => {
                let name = package.parse()?;
                for (_, repo) in repos {
                    results.extend(
                        repo.list_package_versions(&name)?
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
}

impl Ls {
    fn list_recursively(
        &self,
        repos: Vec<(String, spk::storage::RepositoryHandle)>,
    ) -> Result<i32> {
        let mut packages = Vec::new();
        let mut max_repo_name_len = 0;
        for (index, (repo_name, repo)) in repos.iter().enumerate() {
            let num_packages = packages.len();
            match &self.package {
                None => {
                    packages.extend(repo.list_packages()?.into_iter().map(|p| (p, index)));
                }
                Some(package) => {
                    packages.push((package.parse()?, index));
                }
            };
            // Ignore this repo name if it didn't contribute any packages.
            if packages.len() > num_packages {
                max_repo_name_len = max_repo_name_len.max(repo_name.len());
            }
        }
        packages.sort();
        for (package, index) in packages {
            let (repo_name, repo) = repos.get(index).unwrap();
            let mut versions = if package.contains('/') {
                vec![spk::api::parse_ident(&package)?]
            } else {
                let base = spk::api::Ident::from(package);
                repo.list_package_versions(&base.name)?
                    .into_iter()
                    .map(|v| base.with_version(v))
                    .collect()
            };
            versions.sort();
            versions.reverse();
            for pkg in versions {
                let mut builds = repo.list_package_builds(&pkg)?;
                builds.sort();
                for build in builds {
                    if self.verbose > 0 {
                        print!(
                            "{:>width$} ",
                            format!("[{}]", repo_name),
                            width = max_repo_name_len + 2
                        );
                    }
                    println!("{}", self.format_build(&build, repo)?);
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
