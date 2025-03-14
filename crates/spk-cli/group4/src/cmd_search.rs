// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};

use clap::Args;
use colored::Colorize;
use futures::TryStreamExt;
use miette::Result;
use spk_cli_common::{CommandArgs, Run, flags};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::{Deprecate, Package};
use spk_solve::option_map::get_host_options_filters;
use spk_storage::walker::{RepoWalkerBuilder, RepoWalkerItem};

/// Search for packages by name/substring
#[derive(Args)]
pub struct Search {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Show deprecated packages in the output
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

    /// The text/substring to search for in package names
    term: String,
}

#[async_trait::async_trait]
impl Run for Search {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        let width = repos
            .iter()
            .map(|(n, _)| n)
            .map(String::len)
            .max()
            .unwrap_or_default();

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

        let mut deprecated_versions = HashSet::new();
        let mut versions_with_active_builds = HashSet::new();

        let mut repo_map: HashMap<String, &spk_storage::RepositoryHandle> = HashMap::new();
        for (repo_name, repo) in repos.iter() {
            repo_map.insert(repo_name.to_string(), repo);
        }

        let mut repo_walker_builder = RepoWalkerBuilder::new(&repos);
        let repo_walker = repo_walker_builder
            .with_package_name_substring_matching(self.term.clone())
            .with_report_on_versions(true)
            .with_report_on_builds(true)
            .with_report_src_builds(!self.no_src)
            .with_report_deprecated_builds(self.deprecated)
            .with_build_options_matching(filter_by.clone())
            .with_end_of_markers(true)
            .build();
        let mut traversal = repo_walker.walk();

        let mut exit = 1;

        while let Some(item) = traversal.try_next().await? {
            match item {
                RepoWalkerItem::Version(version) => {
                    // Get the repo matching the repo name, this should not be none.
                    let repo = match repo_map.get(&*version.repo_name) {
                        Some(r) => r,
                        None => {
                            println!(
                                "Skipping {}: Error: {} not found in known repos list",
                                version.ident, version.repo_name,
                            );
                            continue;
                        }
                    };

                    // Read the version's recipe to see if it is
                    // deprecated. This doesn't filter it out.
                    match repo.read_recipe(&version.ident).await {
                        Ok(recipe) => {
                            if recipe.is_deprecated() {
                                deprecated_versions.insert(version.ident.clone());
                            }
                        }
                        Err(_) => {
                            // It doesn't have a recipe, but it may
                            // have builds, so unless all the builds
                            // are deprecated, show it if a build
                            // matches the filter. This can happen
                            // when there is a version of a package
                            // that only exists as embedded builds.
                        }
                    }
                }
                RepoWalkerItem::Build(build) => {
                    let version_ident = build.spec.ident().clone().to_version_ident();
                    if !deprecated_versions.contains(&version_ident)
                        && !versions_with_active_builds.contains(&version_ident)
                    {
                        // The Version isn't known to be deprecated
                        // and doesn't have an active build recorded,
                        // so is this build deprecated?
                        if !build.spec.is_deprecated() {
                            // this build isn't deprecated, so the version has an active build
                            versions_with_active_builds.insert(version_ident);
                        }
                    }
                }

                RepoWalkerItem::EndOfVersion(version) => {
                    let mut deprecation_status = "".black();
                    if deprecated_versions.contains(&*version.ident)
                        || !versions_with_active_builds.contains(&*version.ident)
                    {
                        // The version is deprecated or doesn't have
                        // any active builds, i.e. all its builds are
                        // deprecated.
                        if self.deprecated {
                            deprecation_status = " DEPRECATED".red();
                        } else {
                            // Hide the deprecated ones
                            continue;
                        }
                    }

                    exit = 0;
                    println!(
                        "{: <width$} {}{deprecation_status}",
                        version.repo_name,
                        version.ident.format_ident()
                    );
                }
                _ => {}
            }
        }

        Ok(exit)
    }
}

impl CommandArgs for Search {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional arg for a search is the search term
        vec![self.term.clone()]
    }
}
