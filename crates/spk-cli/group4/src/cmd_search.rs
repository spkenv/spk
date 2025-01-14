// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use colored::Colorize;
use miette::Result;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::{Deprecate, Package, VersionIdent};
use spk_solve::option_map::get_host_options_filters;

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

        let mut exit = 1;
        for (repo_name, repo) in repos.iter() {
            for name in repo.list_packages().await? {
                if !name.as_str().contains(&self.term) {
                    continue;
                }
                let versions = repo.list_package_versions(&name).await?;
                let mut ident = VersionIdent::new_zero(name);
                for v in versions.iter() {
                    ident.set_version((**v).clone());

                    let builds = repo.list_package_builds(&ident).await?;
                    if builds.is_empty() {
                        // A version with no builds is treated as if
                        // it does not really exist. This can happen
                        // when a previously published package is
                        // deleted by 'spk rm'.
                        continue;
                    }

                    // Check recipe exists and for deprecation
                    let mut deprecation_status = "".black();
                    match repo.read_recipe(&ident).await {
                        Ok(recipe) => {
                            if recipe.is_deprecated() {
                                if self.deprecated {
                                    deprecation_status = " DEPRECATED".red();
                                } else {
                                    // Hide the deprecated ones
                                    continue;
                                }
                            }
                            // Need to look at the builds to see if
                            // there's one that matches the filters
                            let mut has_a_build_that_matches_the_filter = false;
                            for build in builds {
                                if self.no_src && build.is_source() {
                                    // Filter out source builds
                                    continue;
                                }

                                if let Ok(spec) = repo.read_package(&build).await {
                                    if spec.matches_all_filters(&filter_by) {
                                        has_a_build_that_matches_the_filter = true;
                                        break;
                                    }
                                }
                            }
                            if !has_a_build_that_matches_the_filter {
                                // Hide ones that don't match the filters
                                continue;
                            }
                        }
                        Err(_) => {
                            // It doesn't have a recipe, but it does
                            // have builds, so unless all the builds
                            // are deprecated, show it if a build
                            // matches the filter. This can happen
                            // when there is a version of a package
                            // that only exists as embedded builds.
                            let mut all_builds_deprecated = true;
                            for build in builds {
                                if self.no_src && build.is_source() {
                                    // Filter out source builds
                                    continue;
                                }

                                if let Ok(spec) = repo.read_package(&build).await {
                                    if !spec.is_deprecated() {
                                        if !spec.matches_all_filters(&filter_by) {
                                            // Hide ones that don't match the filters
                                            continue;
                                        }
                                        all_builds_deprecated = false;
                                        break;
                                    }
                                }
                            }
                            if all_builds_deprecated {
                                if self.deprecated {
                                    deprecation_status = " DEPRECATED".red();
                                } else {
                                    continue;
                                }
                            }
                        }
                    };

                    exit = 0;
                    println!(
                        "{repo_name: <width$} {}{deprecation_status}",
                        ident.format_ident()
                    );
                }
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
