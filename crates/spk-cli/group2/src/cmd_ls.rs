// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::sync::Arc;

use clap::{Args, ValueEnum};
use colored::Colorize;
use futures::TryStreamExt;
use miette::Result;
use spfs::Digest;
use spk_cli_common::{CommandArgs, Run, flags};
use spk_schema::foundation::format::{FormatComponents, FormatIdent, FormatOptionMap};
use spk_schema::foundation::ident_component::ComponentSet;
use spk_schema::ident_component::Component;
use spk_schema::name::OptNameBuf;
use spk_schema::option_map::get_host_options_filters;
use spk_schema::{Deprecate, OptionMap, Package, Spec, VersionIdent};
use spk_storage::RepoWalker;
use spk_storage::walker::{RepoWalkerBuilder, RepoWalkerItem, WalkedBuild};
use {spk_config, spk_storage as storage};

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

/// The ways of displaying build options under higher verbosity
#[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum BuildOptionsDisplay {
    All,
    Diff,
}

/// Helper struct used when the one of the build options display flags is used
struct BuildForOutput<'a> {
    pub prefix: String,
    pub build: WalkedBuild<'a>,
    pub components: Option<HashMap<Component, spfs::encoding::Digest>>,
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
    #[clap(long, short = 'R')]
    recursive: bool,

    /// Show the deprecated packages
    #[clap(long, short)]
    deprecated: bool,

    /// Disable the filtering that would only show items that have a
    /// build that matches the current host's host options. This
    /// option can be configured as the default in spk's config file.
    #[clap(long, conflicts_with = "host")]
    no_host: bool,

    /// Enable filtering to only show items that have a build that
    /// matches the current host's host options. This option can be
    /// configured as the default in spk's config file. Enables
    /// --no-src by default.
    #[clap(long)]
    host: bool,

    /// Disable showing items that have any matching build and only
    /// show items with a non-src build that matches the current
    /// host's host options. Using --host will enable this by default.
    #[clap(long, conflicts_with = "src")]
    no_src: bool,

    /// Enable filtering to show items that have any build, including
    /// src ones, that match the current host's host options.
    #[clap(long)]
    src: bool,

    /// Enable the alternative display of build's options, only
    /// applies when verbosity > 0.
    #[clap(long, short = 'b', env = "SPK_LS_BUILD_OPTIONS_DISPLAY", value_enum, default_value_t = BuildOptionsDisplay::All)]
    pub(crate) build_options_display: BuildOptionsDisplay,

    /// Given a name, list versions. Given a name/version list builds.
    ///
    /// If nothing is provided, list all available packages.
    #[clap(name = "NAME[/VERSION]")]
    package: Option<String>,

    #[clap(skip)]
    output: Output,
}

#[async_trait::async_trait]
impl<T: Output> Run for Ls<T> {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let config = spk_config::get_config()?;
        if config.cli.ls.host_filtering {
            if !self.no_host {
                self.host = true;
            }
        } else if !self.host {
            self.no_host = true;
        }

        // Set the default filter to the all current host's host
        // options (--host). --no-host will disable this.
        let filter_by = if !self.no_host && self.host {
            // Using --host enables --no-src by default. But using
            // --src overrides that.
            if !self.src {
                self.no_src = true;
            }
            get_host_options_filters()
        } else {
            None
        };
        tracing::debug!("Filter is: {:?}", filter_by);

        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        let package = self.package.clone();
        let mut repo_walker_builder = RepoWalkerBuilder::new(&repos);
        repo_walker_builder
            .try_with_package_equals(&package)?
            .with_report_on_versions(true)
            .with_report_on_builds(true)
            .with_report_src_builds(!self.no_src)
            .with_report_deprecated_builds(self.deprecated)
            .with_build_options_matching(filter_by.clone());

        if self.recursive {
            let capture_builds =
                self.verbose > 0 && self.build_options_display != BuildOptionsDisplay::All;
            let repo_walker = repo_walker_builder
                .with_end_of_markers(capture_builds)
                .build();

            return self.list_recursively(&repos, &repo_walker).await;
        }

        let results: Vec<String> = match &self.package {
            None => {
                // List all the packages in the repo(s)
                if let Some(_filters) = &filter_by {
                    // With some options filters this needs to walk to
                    // the builds level to do its checks, which can be slow.
                    let repo_walker = repo_walker_builder.build();
                    return self.list_filtered_package_names(&repo_walker).await;
                } else {
                    // Without any options filters this does not need
                    // beyond the package level.
                    let repo_walker = repo_walker_builder.with_report_on_versions(false).build();
                    self.get_all_packages_listing(&repo_walker).await?
                }
            }
            Some(package) if !package.contains('/') => {
                // Given a package name, list all the versions of the package
                let repo_walker = repo_walker_builder.with_end_of_markers(true).build();
                self.get_versions_listing(&repo_walker).await?
            }
            Some(_package) => {
                // Given a package version (or build), list all its builds
                let capture_builds =
                    self.verbose > 0 && self.build_options_display != BuildOptionsDisplay::All;
                let repo_walker = repo_walker_builder
                    .with_end_of_markers(capture_builds)
                    .build();

                return self.list_recursively(&repos, &repo_walker).await;
            }
        };

        // Display the collected results, if any. Some branches
        // display as they go, so results might be empty here.
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
    async fn list_filtered_package_names(&mut self, repo_walker: &RepoWalker<'_>) -> Result<i32> {
        // Outputs packages that have a build that match the walker's
        // filters (usually options host filters). The packages are
        // output as they are returned because checking the builds can
        // be slow.
        let mut set = BTreeSet::new();

        let mut traversal = repo_walker.walk();

        while let Some(item) = traversal.try_next().await? {
            if let RepoWalkerItem::Build(build) = item {
                let package = build.spec.ident().name().to_string();
                if !set.insert(package.clone()) {
                    continue;
                }

                if self.verbose > 0 {
                    self.output
                        .println(format!("[{}] {}", build.repo_name, package));
                } else {
                    self.output.println(package);
                }
            }
        }

        Ok(0)
    }

    async fn get_all_packages_listing(
        &mut self,
        repo_walker: &RepoWalker<'_>,
    ) -> Result<Vec<String>> {
        // Returns a list of output lines that list all the packages
        // found by the walker.
        let mut set = BTreeSet::new();

        let mut traversal = repo_walker.walk();

        while let Some(item) = traversal.try_next().await? {
            if let RepoWalkerItem::Package(package) = item {
                if self.verbose > 0 {
                    set.insert(format!("[{}] {}", package.repo_name, package.name));
                } else {
                    set.insert((*package.name).clone().into());
                }
            }
        }

        Ok(set.into_iter().collect())
    }

    async fn get_versions_listing(&mut self, repo_walker: &RepoWalker<'_>) -> Result<Vec<String>> {
        // Returns a list of output lines that list all the versions
        // of a package founds by the walker.
        let mut active_builds = HashSet::new();
        let mut deprecated_builds = HashSet::new();
        let mut lines = Vec::new();

        let mut traversal = repo_walker.walk();

        while let Some(item) = traversal.try_next().await? {
            match item {
                RepoWalkerItem::Build(build) => {
                    if build.spec.is_deprecated() {
                        deprecated_builds.insert(build.spec.ident().version().clone());
                    } else {
                        active_builds.insert(build.spec.ident().version().clone());
                    }
                }
                RepoWalkerItem::EndOfVersion(version) => {
                    let version_number = version.ident.version();

                    let any_available = active_builds.contains(version_number);
                    let any_deprecated = deprecated_builds.contains(version_number);
                    let all_deprecated = any_deprecated && !any_available;

                    let presentation_version_number = if any_available {
                        match active_builds.get(version_number) {
                            Some(vn) => vn,
                            None => version_number,
                        }
                    } else {
                        version_number
                    };

                    if self.deprecated {
                        // Show deprecated versions with an indication
                        // of how many builds were also deprecated.
                        if all_deprecated {
                            lines.push(format!(
                                "{presentation_version_number} {}",
                                "DEPRECATED".red()
                            ));
                        } else if any_deprecated {
                            lines.push(format!(
                                "{presentation_version_number} {}",
                                "(partially) DEPRECATED".red()
                            ));
                        } else {
                            lines.push(presentation_version_number.to_string());
                        }
                    } else if any_available {
                        lines.push(presentation_version_number.to_string());
                    }
                }
                _ => {}
            }
        }

        Ok(lines)
    }

    async fn list_recursively(
        &mut self,
        repos: &[(String, storage::RepositoryHandle)],
        repo_walker: &RepoWalker<'_>,
    ) -> Result<i32> {
        // Outputs builds that match the walker's filters. The builds
        // are output as match because checking the builds can be slow.

        // Work out the longest repo name, and map the name to their
        // repos for direct lookup for later if need to get build components.
        let mut max_repo_name_len = 0;
        let mut repo_map: HashMap<String, &storage::RepositoryHandle> = HashMap::new();
        for (repo_name, repo) in repos.iter() {
            max_repo_name_len = max_repo_name_len.max(repo_name.len());
            if self.verbose > 1 || self.components {
                repo_map.insert(repo_name.to_string(), repo);
            }
        }

        // Check if one of the alternate build options display is
        // enabled, only for high enough verbosity.
        let capture_builds =
            self.verbose > 0 && self.build_options_display != BuildOptionsDisplay::All;
        let mut builds = Vec::new();

        // Run through all the builds
        let mut traversal = repo_walker.walk();

        while let Some(item) = traversal.try_next().await? {
            match item {
                RepoWalkerItem::Build(build) => {
                    // If going to display the components, get the repo
                    // matching the repo name and look up the build's components.
                    let components = if self.verbose > 1 || self.components {
                        match repo_map.get(build.repo_name) {
                            Some(r) => Some(r.read_components(build.spec.ident()).await?),
                            None => {
                                self.output.warn(format!(
                                    "Skipping {}: Error: {} not found in known repos list",
                                    build.spec.ident(),
                                    build.repo_name
                                ));
                                continue;
                            }
                        }
                    } else {
                        None
                    };

                    let prefix = if self.verbose > 0 {
                        format!(
                            "{:>width$} ",
                            format!("[{}]", build.repo_name),
                            width = max_repo_name_len + 2
                        )
                    } else {
                        "".to_string()
                    };

                    if capture_builds {
                        // Capture this build for options display
                        // processing later.
                        builds.push(BuildForOutput {
                            prefix,
                            build,
                            components,
                        });
                    } else {
                        // Output this build immediately
                        self.output.println(format!(
                            "{prefix}{}",
                            self.format_build(&build.spec, components)?
                        ));
                    }
                }
                // These will only appear if one of the alternate
                // build options output format is enabled at the
                // correct verbosity, because then the main command
                // function will have enabled the end of object
                // markers when it created the walker.
                RepoWalkerItem::EndOfVersion(version) => {
                    if capture_builds && !builds.is_empty() {
                        let lines = self.format_build_lines(version.ident.clone(), &builds)?;
                        for l in lines {
                            self.output.println(l);
                        }
                        builds.clear();
                    }
                }
                _ => (),
            }
        }

        Ok(0)
    }

    fn format_build_lines(
        &self,
        package_version: Arc<VersionIdent>,
        builds: &[BuildForOutput],
    ) -> Result<Vec<String>> {
        let mut results = Vec::new();

        // TODO: make a type?
        let mut counters: BTreeMap<String, HashMap<String, u64>> = BTreeMap::new();
        let mut has_a_source_build = false;

        for build_to_format in builds.iter() {
            let build = &build_to_format.build;

            // First, check for a source build
            if !has_a_source_build && build.spec.ident().is_source() {
                has_a_source_build = true;
                continue;
            }

            // For all the other builds capture and counter their
            // build options and values.
            let options = build.spec.option_values();

            for (name, value) in options.iter() {
                if value.is_empty() {
                    // These options didn't contribute to the build, so ignore them
                    continue;
                }
                let name_entry = counters.entry(name.to_string()).or_default();
                let value_entry = name_entry.entry(value.clone()).or_default();
                *value_entry += 1;
            }
        }

        // Any counter == num builds is an option common to all builds
        // Any counter < num builds is not an option common to all
        // Any counter == 1 is not an option unique to that build
        let mut num_builds: u64 = builds.len().try_into().unwrap();
        if has_a_source_build {
            // Source builds don't have builds options so they
            // aren't counted for working out which options are
            // common to all builds.
            num_builds -= 1;
        }

        // Show the common options first
        let mut common_options: OptionMap = OptionMap::default();
        for (name, value) in counters.iter() {
            if let Some((val, num)) = value.iter().next() {
                // If the first val(ue) doesn't match the number
                // of builds, none of the rest will either because
                // the sum of the value counters will match the
                // number of builds.
                if *num == num_builds {
                    let opt_name = OptNameBuf::new_lossy(&name.clone());
                    common_options.insert(opt_name, val.clone());
                }
            }
        }

        results.push(format!(
            "{} {}",
            format!("Build values common to {package_version} builds:").bold(),
            common_options.format_option_map()
        ));

        // Then show each build with its non-common options
        // highlighted appropriately.
        for build in builds.iter() {
            results.push(format!(
                "{}{}",
                build.prefix,
                self.format_build_with_alt_options(
                    &build.build.spec,
                    build.components.clone(),
                    &counters,
                    num_builds
                )?
            ));
        }

        Ok(results)
    }

    fn format_build_with_alt_options(
        &self,
        spec: &Spec,
        components: Option<HashMap<Component, Digest>>,
        counters: &BTreeMap<String, HashMap<String, u64>>,
        num_builds: u64,
    ) -> Result<String> {
        let mut item = spec.ident().format_ident();
        if spec.is_deprecated() {
            let _ = write!(item, " {}", "DEPRECATED".red());
        }

        // /src packages have no further info to display
        if spec.ident().is_source() {
            return Ok(item);
        }

        // Based on the build options display setting, display the
        // build's uncommon options
        let options = spec.option_values();

        let formatted: Vec<String> = options
            .iter()
            .filter_map(|(name, value)| match self.build_options_display {
                BuildOptionsDisplay::Diff => {
                    // This only shows some
                    if let Some(name_entry) = counters.get(&name.to_string()) {
                        if let Some(value_entry) = name_entry.get(&value.to_string()) {
                            if *value_entry != num_builds {
                                Some(format!("{name}{}{}", "=".dimmed(), value.cyan()))
                            } else {
                                None
                            }
                        } else {
                            // This shouldn't happen
                            None
                        }
                    } else {
                        // This shouldn't happen
                        None
                    }
                }
                BuildOptionsDisplay::All => Some(format!("{name}{}{}", "=".dimmed(), value.cyan())),
            })
            .collect();

        item.push(' ');
        item.push_str(&format!("{{{}}}", formatted.join(", ")));

        // The components
        if let Some(cmpts) = components {
            item.push(' ');
            item.push_str(&ComponentSet::from(cmpts.keys().cloned()).format_components());
        }
        Ok(item)
    }

    fn format_build(
        &self,
        spec: &Spec,
        components: Option<HashMap<Component, Digest>>,
    ) -> Result<String> {
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

        if let Some(cmpts) = components {
            item.push(' ');
            item.push_str(&ComponentSet::from(cmpts.keys().cloned()).format_components());
        }
        Ok(item)
    }
}
