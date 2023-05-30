// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

use std::collections::HashSet;

use anyhow::Result;
use chrono::prelude::*;
use clap::Parser;
use colored::Colorize;
use spfs_cli_common as cli;
use spfs_cli_common::CommandName;

cli::main!(CmdClean);

/// Clean the repository storage of any untracked data
///
/// Untracked data is any data that is not tagged or is not
/// attached to/used by a tagged object. This command also
/// provides semantics for pruning a repository from older
/// tag data to help detach additional data and reduce
/// repository size.
#[derive(Debug, Parser)]
#[clap(name = "spfs-clean")]
pub struct CmdClean {
    #[clap(flatten)]
    pub logging: cli::Logging,

    /// Trigger the clean operation on a remote repository
    #[clap(short, long, group = "repo_data")]
    remote: Option<String>,

    /// Remove the durable upper path component of the named runtime.
    /// If given, this will be the only thing removed.
    #[clap(long, group = "durable_runtimes", value_name = "RUNTIME", conflicts_with_all = ["repo_data"])]
    remove_durable: Option<String>,

    /// The address of the storage being used for runtimes
    ///
    /// Defaults to the current configured local repository.
    #[clap(long, requires = "remove_durable")]
    runtime_storage: Option<url::Url>,

    /// Don't prompt/ask before cleaning the data
    #[clap(long, short)]
    yes: bool,

    /// Don't delete anything, just print what would be deleted (assumes --yes).
    #[clap(long)]
    dry_run: bool,

    /// Prune old tags that have the same target as a more recent version
    #[clap(long = "prune-repeated", group = "repo_data")]
    prune_repeated: bool,

    /// Prune tags older that the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s)
    #[clap(long = "prune-if-older-than", group = "repo_data", value_parser = age_to_date)]
    prune_if_older_than: Option<DateTime<Utc>>,

    /// Always keep data newer than the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s)
    #[clap(long = "keep-if-newer-than", group = "repo_data", value_parser = age_to_date)]
    keep_if_newer_than: Option<DateTime<Utc>>,

    /// Prune tags if there are more than this number in a stream
    #[clap(long = "prune-if-more-than", group = "repo_data")]
    prune_if_more_than: Option<u64>,

    /// Always keep at least this number of tags in a stream
    #[clap(long = "keep-if-less-than", group = "repo_data")]
    keep_if_less_than: Option<u64>,

    /// Do not remove proxies for users that have no additional
    /// hard links.
    ///
    /// Proxies will still be removed if the object is unattached.
    /// This is enabled by default because it is generally considered
    /// safe and can be effective at reducing disk usage.
    #[clap(long = "keep-proxies-with-no-links", group = "repo_data")]
    keep_proxies_with_no_links: bool,

    // The number of concurrent tag stream scanning operations
    // that are buffered and allowed to run concurrently
    #[clap(
        long,
        env = "SPFS_CLEAN_MAX_TAG_STREAM_CONCURRENCY",
        default_value_t = spfs::Cleaner::DEFAULT_TAG_STREAM_CONCURRENCY
    )]
    max_tag_stream_concurrency: usize,

    // The number of concurrent remove operations that are
    // buffered and allowed to run concurrently
    #[clap(
        long,
        env = "SPFS_CLEAN_MAX_REMOVAL_CONCURRENCY",
        default_value_t = spfs::Cleaner::DEFAULT_REMOVAL_CONCURRENCY
    )]
    max_removal_concurrency: usize,

    // The number of concurrent discover/scan operations that are
    // buffered and allowed to run concurrently.
    //
    // This number is applied in a recursive manner, and so can grow
    // exponentially in deeply complex repositories.
    #[clap(
        long,
        env = "SPFS_CLEAN_MAX_DISCOVER_CONCURRENCY",
        default_value_t = spfs::Cleaner::DEFAULT_DISCOVER_CONCURRENCY
    )]
    max_discover_concurrency: usize,
}

impl CommandName for CmdClean {
    fn command_name(&self) -> &'static str {
        "clean"
    }
}

impl CmdClean {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;
        tracing::debug!("spfs clean command called");

        if let Some(runtime_name) = &self.remove_durable {
            tracing::debug!("durable rt name: {}", runtime_name);
            // Remove the durable path associated with the runtime,if
            // there is one. This uses the runtime_storage option
            // because the repo name is not available from the spfs
            // library call that generates spfs clean --remove-durable
            // command line invocations.
            let repo = match &self.runtime_storage {
                Some(address) => spfs::open_repository(address).await?,
                None => config.get_local_repository_handle().await?,
            };
            let storage = spfs::runtime::Storage::new(repo);
            let durable_path = storage.durable_path(runtime_name.clone()).await?;
            tracing::debug!("durable path to remove: {}", durable_path.display());
            if durable_path.exists() {
                // This requires privileges to remove the sub-directories inside the
                // durable upper path's workdir that have 'd---------' permissions
                // from overlayfs.
                std::fs::remove_dir_all(durable_path.clone())
                    .map_err(|err| spfs::Error::RuntimeWriteError(durable_path, err))?;
            }
            return Ok(0);
        }

        let cleaner = spfs::Cleaner::new(&repo)
            .with_reporter(spfs::clean::ConsoleCleanReporter::default())
            .with_dry_run(self.dry_run)
            .with_required_age(chrono::Duration::minutes(15))
            .with_prune_repeated_tags(self.prune_repeated)
            .with_prune_tags_older_than(self.prune_if_older_than)
            .with_keep_tags_newer_than(self.keep_if_newer_than)
            .with_prune_tags_if_version_more_than(self.prune_if_more_than)
            .with_keep_tags_if_version_less_than(self.keep_if_less_than)
            .with_remove_proxies_with_no_links(!self.keep_proxies_with_no_links)
            .with_removal_concurrency(self.max_removal_concurrency)
            .with_discover_concurrency(self.max_discover_concurrency)
            .with_tag_stream_concurrency(self.max_tag_stream_concurrency);

        println!("{}", cleaner.format_plan());
        if !self.dry_run && !self.yes {
            let answer = question::Question::new(
                "This operation may remove data from the repository\n\
                 > Continue with the above plan?",
            )
            .default(question::Answer::NO)
            .show_defaults()
            .confirm();
            match answer {
                question::Answer::YES => {}
                _ => return Ok(2),
            }
        }

        let start = std::time::Instant::now();
        let result = cleaner.prune_all_tags_and_clean().await?;
        let duration = std::time::Instant::now() - start;
        drop(cleaner); // clean up the progress bars

        let spfs::clean::CleanResult {
            visited_tags,
            pruned_tags,
            visited_objects,
            removed_objects,
            visited_payloads,
            removed_payloads,
            visited_renders,
            removed_renders,
            visited_proxies,
            removed_proxies,
            errors,
        } = result;

        println!("{} after {duration:.0?}:", "Finished".bold());
        let removed = if self.dry_run {
            "to remove".yellow().italic()
        } else {
            "removed".red().italic()
        };
        println!(
            "{visited_tags:>12} tags visited     [{:>6} {removed}]",
            pruned_tags.values().map(Vec::len).sum::<usize>()
        );
        println!(
            "{visited_objects:>12} objects visited  [{:>6} {removed}]",
            removed_objects.len()
        );
        println!(
            "{visited_payloads:>12} payloads visited [{:>6} {removed}]",
            removed_payloads.len()
        );
        println!(
            "{visited_renders:>12} renders visited  [{:>6} {removed}]",
            removed_renders.values().map(HashSet::len).sum::<usize>()
        );
        println!(
            "{visited_proxies:>12} proxies visited  [{:>6} {removed}]",
            removed_proxies.values().map(HashSet::len).sum::<usize>()
        );

        if !errors.is_empty() {
            println!("Encountered {} {}", errors.len(), "errors".red());
            println!(" > the 'spfs check' command may provide more details");
        }

        Ok(0)
    }
}

fn age_to_date(age: &str) -> Result<DateTime<Utc>> {
    let (num, postfix) = age.split_at(age.len() - 1);
    let num: i64 = num
        .parse()
        .map_err(|err| spfs::Error::from(format!("{err:?}")))?;
    if num < 0 {
        anyhow::bail!("provided age must be greater than zero: '{age}'");
    }

    match postfix {
        "y" => Ok(Utc::now() - chrono::Duration::weeks(num * 52)),
        "w" => Ok(Utc::now() - chrono::Duration::weeks(num)),
        "d" => Ok(Utc::now() - chrono::Duration::days(num)),
        "h" => Ok(Utc::now() - chrono::Duration::hours(num)),
        "m" => Ok(Utc::now() - chrono::Duration::minutes(num)),
        "s" => Ok(Utc::now() - chrono::Duration::seconds(num)),
        _ => anyhow::bail!("Unknown age postfix: '{postfix}', must be one of y, w, d, h, m, s"),
    }
}
