// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fmt::Arguments;

use clap::Args;
use colored::Colorize;
use futures::TryStreamExt;
use miette::Result;
use spk_cli_common::{CommandArgs, Run, flags};
use spk_storage::{DuSpec, LEVEL_SEPARATOR, extract_du_spec_from_path};

// Number of characters disk size outputs are padded too
const SIZE_WIDTH: usize = 12;

#[cfg(test)]
#[path = "./cmd_du_test.rs"]
mod cmd_du_test;

/// Used for testing to compare the output of the results
pub trait Output: Default + Send + Sync {
    /// A line of output to display.
    fn println(&self, line: Arguments);

    /// A line of output to display as a warning.
    fn warn(&self, line: Arguments);
}

#[derive(Default)]
pub struct Console {}

impl Output for Console {
    fn println(&self, line: Arguments) {
        println!("{line}");
    }

    fn warn(&self, line: Arguments) {
        tracing::warn!("{line}");
    }
}

/// Return the disk usage of a package
#[derive(Args)]
pub struct Du<Output: Default = Console> {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    /// Starting path to calculate the disk usage for. Can be
    /// either in path format:
    /// i.e. REPO/PKG/VERSION/BUILD/:COMPONENTS/dir/to/file
    /// or package request format:
    /// i.e. REPO/PKG:COMPONENTS/VERSION/BUILD/dir/to/file
    #[clap(name = "REPO/PKG/VERSION/BUILD/:COMPONENTS/dir/to/file")]
    pub path: String,

    /// Count sizes many times if hard linked
    #[clap(long, short = 'l')]
    pub count_links: bool,

    /// Lists deprecated packages
    #[clap(long, short = 'd')]
    pub deprecated: bool,

    /// Lists file sizes in human readable format
    #[clap(long, short = 'H')]
    pub human_readable: bool,

    /// Display only a total for each argument
    #[clap(long, short = 's')]
    pub summarize: bool,

    /// Produce the grand total
    #[clap(long, short = 'c')]
    pub total: bool,

    /// Used for testing
    #[clap(skip)]
    pub(crate) output: Output,
}

#[async_trait::async_trait]
impl<T: Output> Run for Du<T> {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        // Work out how the user want to limit du
        let du_spec = extract_du_spec_from_path(&self.path)?;
        tracing::debug!("Disk udage spec: {:?}", du_spec);

        if let Some(ref rn) = du_spec.repo_name {
            // There was a repo name given in the search path so
            // ensure that one is enabled and others are not to focus
            // the repo walker.
            tracing::debug!("Found repo name is du term: {rn}");
            self.repos.enable_repo = vec![rn.clone()];
            if rn == "local" {
                tracing::debug!("Limiting du to local repo");
                self.repos.local_repo_only = true;
            } else {
                tracing::debug!("Excluding local repo");
                self.repos.no_local_repo = true;
            }
        }
        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        if self.summarize {
            self.print_grouped_entries(&repos, &du_spec).await?;
        } else {
            self.print_all_entries(&repos, &du_spec).await?;
        }
        Ok(0)
    }
}

impl<T: Output> CommandArgs for Du<T> {
    fn get_positional_args(&self) -> Vec<String> {
        Vec::new()
    }
}

impl<T: Output> Du<T> {
    fn format_size(&self, size: u64) -> String {
        if self.human_readable {
            spfs::io::format_size(size)
        } else {
            size.to_string()
        }
    }

    fn print_total(&self, total: u64) {
        if self.total {
            self.output.println(format_args!(
                "{:>SIZE_WIDTH$}    total",
                self.format_size(total)
            ));
        }
    }

    async fn print_all_entries(
        &self,
        repos: &Vec<(String, spk_storage::RepositoryHandle)>,
        du_spec: &DuSpec,
    ) -> Result<()> {
        let mut total_size = 0;

        // This uses a DiskUsageRepoWalker that returns individual,
        // non-grouped entries.
        let mut walker_builder = spk_storage::DiskUsageRepoWalkerBuilder::new(repos);
        let mut du_walker = walker_builder
            .with_du_spec(du_spec)?
            .with_count_links(self.count_links)
            .with_deprecated(self.deprecated)
            .build();
        let mut walked = du_walker.individual_entries_du_walk();

        while let Some(du) = walked.try_next().await? {
            total_size += du.entry.size();

            let abs_path = du.flatten_path();
            let joined_path = abs_path.join(&LEVEL_SEPARATOR.to_string());
            let deprecate = if du.deprecated {
                "DEPRECATED".red()
            } else {
                "".into()
            };

            self.output.println(format_args!(
                "{size:>SIZE_WIDTH$}    {joined_path} {deprecate}",
                size = self.format_size(du.entry.size()),
            ));
        }

        self.print_total(total_size);
        Ok(())
    }

    async fn print_grouped_entries(
        &self,
        repos: &Vec<(String, spk_storage::RepositoryHandle)>,
        du_spec: &DuSpec,
    ) -> Result<()> {
        let mut total_size = 0;

        // This uses a DiskUsageRepoWalker that returns grouped
        // entries based on the du spec (paths and depth).
        let mut walker_builder = spk_storage::DiskUsageRepoWalkerBuilder::new(repos);
        let mut du_walker = walker_builder
            .with_du_spec(du_spec)?
            .with_count_links(self.count_links)
            .with_deprecated(self.deprecated)
            .build();
        let mut walked = du_walker.grouped_du_walk();

        while let Some(grouped_entry) = walked.try_next().await? {
            let deprecation_status = if grouped_entry.deprecated {
                "DEPRECATED"
            } else {
                ""
            };

            self.output.println(format_args!(
                "{size:>SIZE_WIDTH$}    {path} {deprecated}",
                path = grouped_entry.grouping,
                size = self.format_size(grouped_entry.size),
                deprecated = deprecation_status.red()
            ));

            total_size += grouped_entry.size;
        }

        self.print_total(total_size);
        Ok(())
    }
}
