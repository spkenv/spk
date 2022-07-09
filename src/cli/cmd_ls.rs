// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{collections::BTreeSet, fmt::Write};

use anyhow::Result;
use clap::Args;
use colored::Colorize;
use spk::api::PkgName;

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

    /// Show the deprecated packages
    #[clap(long, short)]
    deprecated: bool,

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

        let mut results = Vec::new();
        match &self.package {
            None => {
                // List all the packages in the repo(s) - the set
                // provides the sorting, but hides when a package is
                // in multiple repos
                // TODO: should this include the repo name in the output?
                let mut set = BTreeSet::new();
                for (_repo_name, repo) in repos {
                    set.extend(
                        repo.list_packages()?
                            .into_iter()
                            .map(spk::api::PkgNameBuf::into),
                    )
                }
                results = set.into_iter().collect();
            }
            Some(package) if !package.contains('/') => {
                // Given a package name, list all the versions of the package
                let pkgname = PkgName::new(package)?;
                let mut versions = Vec::new();
                for (index, (_, repo)) in repos.iter().enumerate() {
                    versions.extend(
                        repo.list_package_versions(pkgname)?
                            .iter()
                            .cloned()
                            .map(|v| (v, index)),
                    );
                }

                versions.sort_by_key(|v| v.0.clone());
                versions.reverse();

                // Add the sorted versions to the results, in the
                // appropriate format, and after any filtering
                for (version, repo_index) in versions {
                    // TODO: add repo name to output?
                    let (_repo_name, repo) = repos.get(repo_index).unwrap();
                    // TODO: add package name to output?
                    let mut name = String::from(package);
                    name.push('/');
                    name.push_str(&version.to_string());

                    let ident = spk::api::parse_ident(name.clone())?;
                    let spec = repo.read_spec(&ident)?;

                    // TODO: tempted to swap this over to call
                    // format_build, which would add the package name
                    // and more, but also simplify this bringing it
                    // closer to the next Some(package) clause?
                    if self.deprecated {
                        // show deprecated versions
                        if spec.deprecated {
                            results.push(format!("{version} {}", "DEPRECATED".red()));
                            continue;
                        }
                    } else {
                        // don't show deprecated versions
                        if spec.deprecated {
                            continue;
                        }
                    }
                    results.push(version.to_string());
                }
            }
            Some(package) => {
                // Like the None clause, the set provides the sorting
                // but hides when a build is in multiple repos
                // TODO: should this include the repo name in the output?
                let mut set = BTreeSet::new();
                // Given a package version (or build), list all its builds
                let pkg = spk::api::parse_ident(package)?;
                for (_, repo) in repos {
                    for build in repo.list_package_builds(&pkg)? {
                        // Doing this here slows the listing down, but
                        // the spec file is the only place that holds
                        // the deprecation status.
                        let spec = repo.read_spec(&build)?;
                        if spec.deprecated && !self.deprecated {
                            // Hide deprecated packages by default
                            continue;
                        }
                        set.insert(self.format_build(&build, &spec, &repo)?);
                    }
                }
                results = set.into_iter().collect();
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
            let mut versions = if package.as_str().contains('/') {
                vec![spk::api::parse_ident(&package)?]
            } else {
                let base = spk::api::Ident::from(package);
                repo.list_package_versions(&base.name)?
                    .iter()
                    .map(|v| base.with_version((**v).clone()))
                    .collect()
            };
            versions.sort();
            versions.reverse();
            for pkg in versions {
                let mut builds = repo.list_package_builds(&pkg)?;
                builds.sort();
                for build in builds {
                    // Doing this here slows the listing down, but
                    // the spec file is the only place that holds
                    // the deprecation status.
                    let spec = repo.read_spec(&build)?;
                    if spec.deprecated && !self.deprecated {
                        // Hide deprecated packages by default
                        continue;
                    }

                    if self.verbose > 0 {
                        print!(
                            "{:>width$} ",
                            format!("[{}]", repo_name),
                            width = max_repo_name_len + 2
                        );
                    }
                    println!("{}", self.format_build(&build, &spec, repo)?);
                }
            }
        }
        Ok(0)
    }

    fn format_build(
        &self,
        pkg: &spk::api::Ident,
        spec: &spk::api::Spec,
        repo: &spk::storage::RepositoryHandle,
    ) -> Result<String> {
        let mut item = spk::io::format_ident(pkg);
        if spec.deprecated {
            let _ = write!(item, " {}", "DEPRECATED".red());
        }

        // Packages without builds, or /src packages have no further
        // info to display
        if pkg.build.is_none() || pkg.is_source() {
            return Ok(item);
        }

        // Based on the verbosity, display more details for the
        // package build.
        if self.verbose > 0 {
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
