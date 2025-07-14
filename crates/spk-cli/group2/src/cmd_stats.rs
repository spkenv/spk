// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::BTreeMap;
use std::time::Instant;

use clap::Args;
use futures::TryStreamExt;
use miette::Result;
use spfs::io::Pluralize;
use spk_cli_common::{CommandArgs, Run, flags};
use spk_schema::foundation::name::PkgNameBuf;
use spk_schema::version::Version;
use spk_schema::{Deprecate, Package};
use spk_solve::BuildIdent;
use spk_solve::option_map::get_host_options_filters;
use spk_storage::RepoWalker;
use spk_storage::walker::{RepoWalkerBuilder, RepoWalkerItem};

use crate::cmd_ls::{Console, Output};

#[cfg(test)]
#[path = "./cmd_stats_test.rs"]
mod cmd_stats_test;

pub const ONE_PACKAGE_WAIT_MESSAGE: &str = "This may take a few seconds, please wait ...";
pub const ALL_PACKAGES_WAIT_MESSAGE: &str = "This may take a few minutes, please wait ...";

// Counters for stats about a version's builds
#[derive(Default, Debug)]
struct VersionStats {
    num_builds: u64,
    num_deprecated_builds: u64,
    num_src_builds: u64,
}

// Total and average stats for a package
#[derive(Default, Debug)]
struct PackageStats {
    package_name: String,
    total_versions: u64,
    total_builds: u64,
    builds_per_version: u64,
}

// Collection of counters and other stats for a package
#[derive(Default, Debug)]
struct PackageStatsCollector {
    // TODO: add embedded builds/versions counting
    num_builds: u64,
    num_deprecated_builds: u64,
    num_non_deprecated_src_builds: u64,

    // This doesn't keep the individual builds, just a counter for the
    // number in each version.
    package_version_num_builds: BTreeMap<PkgNameBuf, BTreeMap<Version, VersionStats>>,
    repos: BTreeMap<String, u64>,
}

impl PackageStatsCollector {
    pub fn new() -> Self {
        Self {
            num_builds: 0,
            num_deprecated_builds: 0,
            num_non_deprecated_src_builds: 0,
            package_version_num_builds: Default::default(),
            repos: Default::default(),
        }
    }

    pub fn increment_total_builds(&mut self) {
        self.num_builds += 1;
    }

    pub fn increment_total_deprecated_builds(&mut self) {
        self.num_deprecated_builds += 1;
    }

    pub fn increment_total_non_deprecated_src_builds(&mut self) {
        self.num_non_deprecated_src_builds += 1;
    }

    pub fn increment_package_version_builds(&mut self, ident: &BuildIdent, is_deprecated: bool) {
        let package_entry = self
            .package_version_num_builds
            .entry(ident.name().into())
            .or_default();
        let versions_entry = package_entry.entry(ident.version().clone()).or_default();
        versions_entry.num_builds += 1;

        if is_deprecated {
            versions_entry.num_deprecated_builds += 1;
        }
        if ident.is_source() {
            versions_entry.num_src_builds += 1;
        }
    }

    pub fn increment_repo_packages(&mut self, repo_name: String) {
        let repo_entry = self.repos.entry(repo_name).or_default();
        *repo_entry += 1;
    }

    pub fn total_packages(&self) -> u64 {
        self.package_version_num_builds.len() as u64
    }

    pub fn total_versions(&self) -> u64 {
        self.package_version_num_builds
            .values()
            .map(|v| v.len() as u64)
            .sum()
    }

    pub fn total_deprecated_versions(&self) -> u64 {
        self.package_version_num_builds
            .values()
            .map(|v| {
                v.iter()
                    .map(|(_v, e)| {
                        // Count versions with only deprecated builds
                        if e.num_builds == e.num_deprecated_builds {
                            1
                        } else {
                            0
                        }
                    })
                    .sum::<u64>()
            })
            .sum()
    }

    pub fn total_active_versions(&self) -> u64 {
        self.total_versions() - self.total_deprecated_versions()
    }

    pub fn total_builds(&self) -> u64 {
        self.num_builds
    }

    pub fn total_deprecated_builds(&self) -> u64 {
        self.num_deprecated_builds
    }

    pub fn total_source_builds(&self) -> u64 {
        self.num_non_deprecated_src_builds
    }

    // TODO: active maybe isn't the best name, it's non-src,
    // non-deprecated, so maybe 'available' or 'usable' would be a
    // better description?
    pub fn total_active_builds(&self) -> u64 {
        self.num_builds - self.num_deprecated_builds - self.num_non_deprecated_src_builds
    }

    pub fn active_builds_ratio(&self) -> f64 {
        if self.num_builds == 0 {
            return 0.0;
        }
        self.total_active_builds() as f64 / self.num_builds as f64
    }

    pub fn deprecated_builds_ratio(&self) -> f64 {
        if self.num_builds == 0 {
            return 0.0;
        }
        self.num_deprecated_builds as f64 / self.num_builds as f64
    }

    pub fn source_builds_ratio(&self) -> f64 {
        if self.num_builds == 0 {
            return 0.0;
        }
        self.num_non_deprecated_src_builds as f64 / self.num_builds as f64
    }

    pub fn average_active_builds_per_package(&self) -> f64 {
        let total_packages = self.total_packages();
        if total_packages == 0 {
            return 0.0;
        }
        ((self.num_builds - self.num_deprecated_builds) / total_packages) as f64
    }

    // This includes deprecated and /src
    pub fn average_active_versions_per_package(&self) -> f64 {
        let total_packages = self.total_packages();
        if total_packages == 0 {
            return 0.0;
        }
        (self.total_active_versions() / total_packages) as f64
    }

    pub fn versions_builds_averages_for_all_packages(&self) -> Vec<PackageStats> {
        self.package_version_num_builds
            .iter()
            .map(|(p, versions)| {
                let total_versions: u64 = versions.len() as u64;
                let total_builds: u64 = versions.iter().map(|(_v, e)| e.num_builds).sum();
                let builds_per_version: u64 = total_builds / total_versions;

                PackageStats {
                    package_name: p.to_string(),
                    total_versions,
                    total_builds,
                    builds_per_version,
                }
            })
            .collect()
    }

    pub fn versions_active_builds_averages_for_all_packages(&self) -> Vec<PackageStats> {
        self.package_version_num_builds
            .iter()
            .map(|(p, versions)| {
                let active_vers: u64 = versions
                    .iter()
                    .map(|(_v, e)| {
                        if e.num_builds == e.num_deprecated_builds {
                            // This version is deprecated, it has no active builds
                            0
                        } else {
                            1
                        }
                    })
                    .sum::<u64>();

                let active_builds: u64 = versions
                    .iter()
                    .map(|(_v, e)| e.num_builds - e.num_deprecated_builds)
                    .sum();

                let active_builds_per_ver: u64 = if active_vers == 0 {
                    0
                } else {
                    active_builds / active_vers
                };

                PackageStats {
                    package_name: p.to_string(),
                    total_versions: active_vers,
                    total_builds: active_builds,
                    builds_per_version: active_builds_per_ver,
                }
            })
            .collect()
    }

    /// Returns a list of repo name, build count pairs.
    pub fn build_counts_for_all_repos(&self) -> Vec<(String, u64)> {
        self.repos
            .iter()
            .map(|(name, number)| (name.to_string(), *number))
            .collect()
    }
}

/// Show package, version, build stats for one or more repositories
#[derive(Args)]
#[clap(visible_alias = "report")]
pub struct Stats<Output: Default = Console> {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Set the limit for how many packages with the worst builds to
    /// versions ratio (average branching factor) to show.
    #[clap(long, env = "SPK_STATS_SHOW_TOP_N", default_value_t = 10)]
    pub show_top: usize,

    /// Include deprecated builds and versions in the ratios
    #[clap(long, short)]
    pub(crate) deprecated: bool,

    /// Disable the filtering that would only show items that have a
    /// build that matches the current host's host options. This
    /// option can be configured as the default in spk's config file.
    #[clap(long, conflicts_with = "host")]
    pub(crate) no_host: bool,

    /// Enable filtering to only show items that have a build that
    /// matches the current host's host options. This option can be
    /// configured as the default in spk's config file.
    #[clap(long)]
    pub(crate) host: bool,

    /// A name of the package to gather stats on. When not given, this
    /// command gathers stats on all packages in the repos.
    #[clap(name = "NAME")]
    package: Option<String>,

    #[clap(skip)]
    pub(crate) output: Output,
}

#[async_trait::async_trait]
impl<T: Output> Run for Stats<T> {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let start = Instant::now();

        let config = spk_config::get_config()?;
        if config.cli.ls.host_filtering {
            if !self.no_host {
                self.host = true;
            }
        } else if !self.host {
            self.no_host = true;
        }

        let repos = self.repos.get_repos_for_non_destructive_operation().await?;
        let repo_names: Vec<_> = repos.iter().map(|(name, _)| name.to_string()).collect();
        let max_repo_name_len = repo_names
            .iter()
            .fold(usize::MIN, |acc, item| acc.max((*item).len()));

        // TODO: this is repeated in spk search, move to flags.rs?
        // along with the config+host checks done above?  Set the
        // default filter to the all current host's host options
        // (--host). --no-host will disable this.
        let filter_by = if !self.no_host && self.host {
            get_host_options_filters()
        } else {
            None
        };
        tracing::trace!("Filter is: {:?}", filter_by);

        self.output.println(format!(
            "Gathering stats on packages, versions, and builds in the {} {}.",
            repo_names.join(", "),
            "repo".pluralize(repo_names.len())
        ));
        if self.package.is_some() {
            self.output.println(ONE_PACKAGE_WAIT_MESSAGE.to_string());
        } else {
            self.output.println(ALL_PACKAGES_WAIT_MESSAGE.to_string());
        }

        // Gather the stats
        let mut repo_walker_builder = RepoWalkerBuilder::new(&repos);
        let repo_walker = repo_walker_builder
            .try_with_package_equals(&self.package)?
            .with_report_on_versions(true)
            .with_report_on_builds(true)
            .with_report_src_builds(true)
            .with_report_deprecated_builds(true)
            // TODO: should this be true so we can count them? if so,
            // remember to remove them from the builds total or
            // there will be double counting.
            .with_report_embedded_builds(false)
            .with_build_options_matching(filter_by.clone())
            .build();

        let stats = self.gather_stats(&repo_walker).await?;

        // Release the references held in the walker objects to allow
        // the mutable reference in show_stats()
        drop(repo_walker);
        drop(repo_walker_builder);

        self.show_stats(&repo_names, max_repo_name_len, &stats, &start);
        Ok(0)
    }
}

impl<T: Output> Stats<T> {
    async fn gather_stats(&self, repo_walker: &RepoWalker<'_>) -> Result<PackageStatsCollector> {
        let mut stats = PackageStatsCollector::new();

        let mut traversal = repo_walker.walk();

        while let Some(item) = traversal.try_next().await? {
            if let RepoWalkerItem::Build(build) = item {
                // The build data is used to gather that stats. The
                // other objects emitted from the walker are ignored.
                let ident = build.spec.ident();

                if build.spec.is_deprecated() {
                    stats.increment_total_deprecated_builds();
                } else if ident.is_source() {
                    stats.increment_total_non_deprecated_src_builds();
                }
                stats.increment_total_builds();

                stats.increment_package_version_builds(ident, build.spec.is_deprecated());

                stats.increment_repo_packages(build.repo_name.to_string());
            }
        }

        Ok(stats)
    }

    fn show_stats(
        &mut self,
        repo_names: &[String],
        max_repo_name_len: usize,
        stats: &PackageStatsCollector,
        start: &Instant,
    ) {
        self.output.println(format!(
            "------- Spk stats for {} {} -------",
            repo_names.join(", "),
            "repo".pluralize(repo_names.len())
        ));
        for (repo_name, build_count) in stats.build_counts_for_all_repos().iter() {
            if let Some(package) = &self.package {
                self.output.println(format!(
                    "{repo_name:max_repo_name_len$} repo has {build_count} {} of '{package}'",
                    "build".pluralize(repo_names.len()),
                ));
            } else {
                self.output.println(format!(
                    "{repo_name:max_repo_name_len$} repo has {build_count} {} of packages",
                    "build".pluralize(repo_names.len()),
                ));
            }
        }
        self.output.println("".to_string());
        let max_width = stats.total_builds().to_string().len();
        self.output.println(format!(
            "Total packages   : {:max_width$}",
            stats.total_packages()
        ));
        self.output.println(format!(
            "Total versions   : {:max_width$}  ({} deprecated {:0.0}%)",
            stats.total_versions(),
            stats.total_deprecated_versions(),
            if stats.total_versions() == 0 {
                0.0
            } else {
                stats.total_deprecated_versions() as f64 / stats.total_versions() as f64 * 100.0
            }
        ));
        self.output.println(format!(
            "Total builds     : {:max_width$}  (100%)",
            stats.total_builds()
        ));
        self.output.println(format!(
            "Deprecated builds: {:max_width$}  ({:3.0}%)",
            stats.total_deprecated_builds(),
            stats.deprecated_builds_ratio() * 100.0
        ));
        self.output.println(format!(
            "Source builds    : {:max_width$}  ({:3.0}%)",
            stats.total_source_builds(),
            stats.source_builds_ratio() * 100.0
        ));
        self.output.println(format!(
            "Active builds    : {:max_width$}  ({:3.0}%)",
            stats.total_active_builds(),
            stats.active_builds_ratio() * 100.0
        ));
        self.output.println(format!(
            "Average usable versions per package: {:0.0}",
            stats.average_active_versions_per_package()
        ));
        self.output.println(format!(
            "Average usable   builds per package: {:0.0}  (branching factor)",
            stats.average_active_builds_per_package()
        ));

        // Decide whether to include deprecated builds and versions or not
        let mut packages_data = if self.deprecated {
            stats.versions_builds_averages_for_all_packages()
        } else {
            stats.versions_active_builds_averages_for_all_packages()
        };

        let desc = if self.deprecated {
            "incl. deprecated"
        } else {
            "usable"
        };

        self.output.println("".to_string());
        if self.package.is_some() {
            // Only have the stats on one package
            for package_stats in packages_data.iter() {
                let num_vers = package_stats.total_versions;
                let num_builds = package_stats.total_builds;
                let avg_builds_per_ver = package_stats.builds_per_version;

                let width = num_builds.to_string().len();
                self.output
                    .println(format!("{}:", package_stats.package_name));
                self.output
                    .println(format!(" {num_builds:width$} builds ({desc})"));
                self.output
                    .println(format!(" {num_vers:width$} versions ({desc})"));
                self.output.println(format!(
                    " {avg_builds_per_ver:width$} builds per version ({num_builds}/{num_vers})"
                ));
            }
        } else {
            // For stats on all packages, show the top N highest build
            // to versions ratio packages.
            let number_to_show = if self.show_top == 0 {
                packages_data.len()
            } else {
                self.show_top
            };

            // Versions top N
            packages_data.sort_by_key(|p| p.total_versions);
            packages_data.reverse();

            let mut name_width = 0;
            let mut number_width = 0;
            for package_stats in packages_data
                .iter()
                .filter(|p| p.total_versions > 1)
                .take(number_to_show)
            {
                name_width = name_width.max(package_stats.package_name.to_string().len());
                number_width = number_width.max(package_stats.total_versions.to_string().len());
            }

            self.output.println(format!(
                "Top {number_to_show} packages with the most versions {desc}:"
            ));

            for package_stats in packages_data.iter().take(number_to_show) {
                self.output.println(format!(
                    " - {:name_width$} : {:number_width$} {}",
                    package_stats.package_name,
                    package_stats.total_versions,
                    "version".pluralize(package_stats.total_versions)
                ));
            }
            self.output.println("".to_string());

            // Builds per Version top N
            packages_data.sort_by_key(|p| p.builds_per_version);
            packages_data.reverse();

            name_width = 0;
            number_width = 0;
            let mut ratio_width = 0;
            for package_stats in packages_data
                .iter()
                .filter(|e| e.total_versions > 1)
                .take(number_to_show)
            {
                name_width = name_width.max(package_stats.package_name.to_string().len());
                number_width = number_width.max(package_stats.builds_per_version.to_string().len());
                // Ratio widths include 3 extra characters for the (,
                // /, and ) in the output, e.g. (54/5)
                ratio_width = ratio_width.max(
                    package_stats.total_versions.to_string().len()
                        + package_stats.total_builds.to_string().len()
                        + 3,
                );
            }

            self.output.println(format!(
                "Top {number_to_show} packages with the most total builds per version {desc}:"
            ));

            for package_stats in packages_data
                .iter()
                .filter(|e| e.total_versions > 1)
                .take(number_to_show)
            {
                let num_vers = package_stats.total_versions;
                let num_builds = package_stats.total_builds;
                let ratio = format!("({num_builds}/{num_vers})");
                self.output.println(format!(" - {:name_width$} : {:number_width$} builds/version {ratio:ratio_width$} {desc}",
                                            package_stats.package_name,
                                            package_stats.builds_per_version,
                ));
            }
        }

        self.output.println("".to_string());
        self.output.println(format!(
            "Time taken: {:0.2} seconds  ({:0.6} secs/build)",
            start.elapsed().as_secs_f64(),
            start.elapsed().as_secs_f64() / stats.total_builds() as f64
        ));
    }
}

impl CommandArgs for Stats {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for stats are the packages
        match &self.package {
            Some(pkg) => vec![pkg.clone()],
            None => vec![],
        }
    }
}
