// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{BTreeSet, HashSet};
use std::fmt::Write;

use clap::Args;
use colored::Colorize;
use miette::{miette, Error, Result};
use nom::combinator::all_consuming;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::foundation::format::{FormatComponents, FormatIdent, FormatOptionMap};
use spk_schema::foundation::ident_component::ComponentSet;
use spk_schema::foundation::name::{PkgName, PkgNameBuf};
use spk_schema::ident::{parse_ident, AnyIdent};
use spk_schema::ident_ops::parsing::{ident_parts, IdentParts, KNOWN_REPOSITORY_NAMES};
use spk_schema::name::OptNameBuf;
use spk_schema::spec_ops::WithVersion;
use spk_schema::{Deprecate, OptionMap, Package, Spec};
use spk_storage as storage;

#[cfg(test)]
#[path = "./cmd_ls_test.rs"]
mod cmd_ls_test;

pub trait Output: Default + Send + Sync {
    /// A line of output to display.
    fn println(&mut self, line: String);

    /// A line of output to display as a warning.
    fn warn(&mut self, line: String);
}

#[derive(Default)]
pub struct Console {}

impl Output for Console {
    fn println(&mut self, line: String) {
        println!("{line}");
    }

    fn warn(&mut self, line: String) {
        tracing::warn!("{line}");
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum FilterOperator {
    // name=value and name must be present and value must match
    MustMatchNameValue,
    // name?=value and if name is present then value must match, if
    // name is not present it is considered a match.
    OkIfNameMissing,
}

#[derive(Debug, Clone)]
pub(crate) struct OptFilter {
    pub(crate) name: OptNameBuf,
    pub(crate) op: FilterOperator,
    pub(crate) value: String,
}

impl OptFilter {
    pub fn matches(&self, options: &OptionMap) -> bool {
        if let Some(v) = options.get(&self.name) {
            self.value == *v
        } else if self.op == FilterOperator::MustMatchNameValue {
            // name=value filters must have the name (and value)
            // to match, not having the name means it does not match
            false
        } else {
            // name?=value filter matches when the name is not
            // present. Note this means /src builds will always match
            // this kind of filter because /src builds have empty
            // options sets.
            true
        }
    }
}

impl std::str::FromStr for OptFilter {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let mut op = FilterOperator::MustMatchNameValue;

        // TODO: if this gets more complicated it may need to use a
        // proper parser.
        let (name, value) = if value.contains("?=") {
            op = FilterOperator::OkIfNameMissing;
            value
                .split_once("?=")
                .ok_or_else(|| {
                    miette!("Invalid option filter: {value} (should be in the form NAME?=VALUE or NAME=VALUE)")
                })
                .and_then(|(name, value)| Ok((OptNameBuf::try_from(name)?, value)))?
        } else {
            value
                .split_once('=')
                .or_else(|| value.split_once(':'))
                .ok_or_else(|| {
                    miette!("Invalid option filter: {value} (should be in the form NAME=VALUE or NAME?=VALUE)")
                })
                .and_then(|(name, value)| Ok((OptNameBuf::try_from(name)?, value)))?
        };
        Ok(Self {
            name,
            op,
            value: value.to_string(),
        })
    }
}

/// List packages in one or more repositories
#[derive(Args)]
#[clap(visible_alias = "list")]
pub struct Ls<Output: Default = Console> {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Show available package components in the output
    #[clap(long, short)]
    components: bool,

    /// Recursively list all package versions and builds
    #[clap(long)]
    recursive: bool,

    /// Show the deprecated packages
    #[clap(long, short)]
    deprecated: bool,

    /// Only show packages with builds that match the filter,
    /// e.g. 'distro=centos'. You can use '?=' instead of '=' if the
    /// filter should only used if the opt is present in builds.
    #[clap(long, name = "OPT=VALUE")]
    filter_by: Option<OptFilter>,

    // TODO: disable the always check the host options setting, which I haven't implemented yet
    // #[cla[(long)]
    // nohost: bool
    /// Given a name, list versions. Given a name/version list builds.
    ///
    /// If nothing is provided, list all available packages.
    #[clap(name = "NAME[/VERSION]")]
    package: Option<String>,

    #[clap(skip)]
    pub(crate) output: Output,
}

#[async_trait::async_trait]
impl<T: Output> Run for Ls<T> {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        if self.recursive {
            return self.list_recursively(repos).await;
        }

        let mut results = Vec::new();
        match &self.package {
            None => {
                // List all the packages in the repo(s) - the set
                // provides the sorting, but hides when a package is
                // in multiple repos
                // TODO: should this include the repo name in the output?
                let mut set = BTreeSet::new();
                if let Some(_filter) = &self.filter_by {
                    set.extend(
                        self.filter_all_top_level_packages(repos)
                            .await?
                            .into_iter()
                            .map(PkgNameBuf::into),
                    );
                } else {
                    // Simpler without a filter
                    for (_repo_name, repo) in repos {
                        set.extend(
                            repo.list_packages()
                                .await?
                                .into_iter()
                                .map(PkgNameBuf::into),
                        )
                    }
                }
                results = set.into_iter().collect();
            }
            Some(package) if !package.contains('/') => {
                // Given a package name, list all the versions of the package
                let pkgname = PkgName::new(package)?;
                let mut versions = Vec::new();
                for (index, (_, repo)) in repos.iter().enumerate() {
                    versions.extend(
                        repo.list_package_versions(pkgname)
                            .await?
                            .iter()
                            .map(|v| ((**v).clone(), index)),
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

                    let ident = parse_ident(name.clone())?;

                    // In order to honor showing or hiding deprecated builds,
                    // inventory the builds of this version (do not depend on
                    // the existence of a "version spec").

                    let mut builds = repo.list_package_builds(ident.as_version()).await?;
                    if builds.is_empty() {
                        // Does a version with no builds really exist?
                        continue;
                    }

                    let mut builds_remaining = false;
                    let mut any_deprecated = false;
                    let mut any_not_deprecated = false;
                    while let Some(build) = builds.pop() {
                        match repo.read_package(&build).await {
                            Ok(spec) => {
                                if let Some(filter) = &self.filter_by {
                                    if !filter.matches(&spec.option_values()) {
                                        // Skip this one
                                        continue;
                                    }
                                }

                                builds_remaining = true;

                                if spec.is_deprecated() {
                                    any_deprecated = true;
                                } else {
                                    any_not_deprecated = true;
                                }
                            }
                            Err(err) => {
                                self.output
                                    .warn(format!("Error reading spec for {build}: {err}"));
                            }
                        }
                        if any_not_deprecated && any_deprecated {
                            break;
                        }
                    }

                    if !builds_remaining {
                        // All the builds of this version were filtered out
                        continue;
                    }

                    let all_deprecated = any_deprecated && !any_not_deprecated;

                    // TODO: tempted to swap this over to call
                    // format_build, which would add the package name
                    // and more, but also simplify this bringing it
                    // closer to the next Some(package) clause?
                    if self.deprecated {
                        // show deprecated versions
                        if all_deprecated {
                            results.push(format!("{version} {}", "DEPRECATED".red()));
                            continue;
                        } else if any_deprecated {
                            results.push(format!("{version} {}", "(partially) DEPRECATED".red()));
                            continue;
                        }
                    } else {
                        // don't show deprecated versions
                        if all_deprecated {
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
                let pkg = parse_ident(package)?;
                for (_, repo) in repos {
                    for build in repo.list_package_builds(pkg.as_version()).await? {
                        // Doing this here slows the listing down, but
                        // the spec file is the only place that holds
                        // the deprecation status.
                        let spec = match repo.read_package(&build).await {
                            Ok(spec) => spec,
                            Err(err) => {
                                self.output.warn(format!("Skipping {build}: {err}"));
                                continue;
                            }
                        };

                        if let Some(filter) = &self.filter_by {
                            if !filter.matches(&spec.option_values()) {
                                // Skip this one
                                continue;
                            }
                        }

                        if spec.is_deprecated() && !self.deprecated {
                            // Hide deprecated packages by default
                            continue;
                        }
                        set.insert(self.format_build(&spec, &repo).await?);
                    }
                }
                results = set.into_iter().collect();
            }
        }

        for item in results {
            self.output.println(item.to_string());
        }
        Ok(0)
    }
}

impl<T: Output> CommandArgs for Ls<T> {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for a ls are the packages
        match &self.package {
            Some(pkg) => vec![pkg.clone()],
            None => vec![],
        }
    }
}

impl<T: Output> Ls<T> {
    async fn list_recursively(
        &mut self,
        repos: Vec<(String, storage::RepositoryHandle)>,
    ) -> Result<i32> {
        let search_term = self
            .package
            .as_ref()
            .map(|ident| {
                all_consuming(ident_parts::<nom_supreme::error::ErrorTree<_>>(
                    &KNOWN_REPOSITORY_NAMES,
                ))(ident)
                .map(|(_, parts)| parts)
                .map_err(|err| match err {
                    nom::Err::Error(e) | nom::Err::Failure(e) => {
                        miette!(e.to_string())
                    }
                    nom::Err::Incomplete(_) => unreachable!(),
                })
            })
            .transpose()?;

        let mut packages = Vec::new();
        let mut max_repo_name_len = 0;
        for (index, (repo_name, repo)) in repos.iter().enumerate() {
            let num_packages = packages.len();
            match &search_term {
                None => {
                    packages.extend(repo.list_packages().await?.into_iter().map(|p| (p, index)));
                }
                Some(IdentParts {
                    repository_name: Some(name),
                    ..
                }) if name != repo_name => continue,
                Some(IdentParts { pkg_name, .. }) => {
                    packages.push((pkg_name.parse()?, index));
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
            let mut versions = {
                let base = AnyIdent::from(package);
                repo.list_package_versions(base.name())
                    .await?
                    .iter()
                    .filter_map(|v| match search_term {
                        Some(IdentParts {
                            version_str: Some(version),
                            ..
                        }) if version != v.to_string() => None,
                        _ => Some(base.with_version((**v).clone())),
                    })
                    .collect::<Vec<_>>()
            };
            versions.sort();
            versions.reverse();
            for pkg in versions {
                let mut builds = repo.list_package_builds(pkg.as_version()).await?;
                builds.sort();
                for build in builds {
                    if let Some(IdentParts {
                        build_str: Some(search_build),
                        ..
                    }) = search_term
                    {
                        if build.build().to_string() != search_build {
                            continue;
                        }
                    }

                    // Doing this here slows the listing down, but
                    // the spec file is the only place that holds
                    // the deprecation status.
                    let spec = match repo.read_package(&build).await {
                        Ok(spec) => spec,
                        Err(err) => {
                            self.output.warn(format!("Skipping {build}: {err}"));
                            continue;
                        }
                    };

                    if let Some(filter) = &self.filter_by {
                        if !filter.matches(&spec.option_values()) {
                            // Skip this one
                            continue;
                        }
                    }

                    if spec.is_deprecated() && !self.deprecated {
                        // Hide deprecated packages by default
                        continue;
                    }

                    if self.verbose > 0 {
                        print!(
                            "{:>width$} ",
                            format!("[{repo_name}]"),
                            width = max_repo_name_len + 2
                        );
                    }
                    self.output
                        .println((self.format_build(&spec, repo).await?).to_string());
                }
            }
        }
        Ok(0)
    }

    async fn filter_all_top_level_packages(
        &mut self,
        repos: Vec<(String, storage::RepositoryHandle)>,
    ) -> Result<HashSet<PkgNameBuf>> {
        let mut packages = Vec::new();
        for (index, (_repo_name, repo)) in repos.iter().enumerate() {
            packages.extend(repo.list_packages().await?.into_iter().map(|p| (p, index)));
        }

        let mut results = HashSet::new();
        for (package, index) in packages {
            if results.contains(&package) {
                continue;
            }

            let (_repo_name, repo) = repos.get(index).unwrap();

            let versions = {
                let base = AnyIdent::from(package.clone());
                repo.list_package_versions(base.name())
                    .await?
                    .iter()
                    .map(|v| base.with_version((**v).clone()))
                    .collect::<Vec<_>>()
            };

            for pkg in versions {
                let mut found_a_match = false;
                for build in repo.list_package_builds(pkg.as_version()).await? {
                    let spec = match repo.read_package(&build).await {
                        Ok(spec) => spec,
                        Err(err) => {
                            self.output.warn(format!("Skipping {build}: {err}"));
                            continue;
                        }
                    };

                    if let Some(filter) = &self.filter_by {
                        if !filter.matches(&spec.option_values()) {
                            // Skip this one
                            continue;
                        }
                    }

                    if spec.is_deprecated() && !self.deprecated {
                        // Hide deprecated packages by default
                        continue;
                    }

                    // One build passing the filters is enough for this
                    // version to be counted as matching.
                    found_a_match = true;
                    break;
                }

                // One version with a matching build is enough for
                // this package to be counted has matching
                if found_a_match {
                    results.insert(package);
                    break;
                }
            }
        }

        Ok(results)
    }

    async fn format_build(&self, spec: &Spec, repo: &storage::RepositoryHandle) -> Result<String> {
        let mut item = spec.ident().format_ident();
        if spec.is_deprecated() {
            let _ = write!(item, " {}", "DEPRECATED".red());
        }

        // /src packages have no further info to display
        if spec.ident().is_source() {
            return Ok(item);
        }

        // Based on the verbosity, display more details for the
        // package build.
        if self.verbose > 0 {
            let options = spec.option_values();
            item.push(' ');
            item.push_str(&options.format_option_map());
        }

        if self.verbose > 1 || self.components {
            let cmpts = repo.read_components(spec.ident()).await?;
            item.push(' ');
            item.push_str(&ComponentSet::from(cmpts.keys().cloned()).format_components());
        }
        Ok(item)
    }
}
