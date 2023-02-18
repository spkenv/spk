// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

use anyhow::Result;
use chrono::prelude::*;
use clap::Parser;
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
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Trigger the clean operation on a remote repository
    #[clap(short, long)]
    remote: Option<String>,

    /// Don't prompt/ask before cleaning the data
    #[clap(long, short)]
    yes: bool,

    /// Don't delete anything, just print what would be deleted (assumes --yes).
    #[clap(long)]
    dry_run: bool,

    /// Prune old tags that have the same target as a more recent version
    #[clap(long = "prune-repeated")]
    prune_repeated: bool,

    /// Prune tags older that the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s)
    #[clap(long = "prune-if-older-than", value_parser = age_to_date)]
    prune_if_older_than: Option<DateTime<Utc>>,

    /// Always keep data newer than the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s)
    #[clap(long = "keep-if-newer-than", value_parser = age_to_date)]
    keep_if_newer_than: Option<DateTime<Utc>>,

    /// Prune tags if there are more than this number in a stream
    #[clap(long = "prune-if-more-than")]
    prune_if_more_than: Option<u64>,

    /// Always keep at least this number of tags in a stream
    #[clap(long = "keep-if-less-than")]
    keep_if_less_than: Option<u64>,
}

impl CommandName for CmdClean {
    fn command_name(&self) -> &'static str {
        "clean"
    }
}

impl CmdClean {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let cleaner = spfs::Cleaner::new(&repo)
            .with_reporter(spfs::clean::ConsoleCleanReporter::default())
            .with_dry_run(self.dry_run)
            .with_required_age(chrono::Duration::minutes(15))
            .with_prune_repeated_tags(self.prune_repeated)
            .with_prune_tags_older_than(self.prune_if_older_than)
            .with_keep_tags_newer_than(self.keep_if_newer_than)
            .with_prune_tags_if_version_more_than(self.prune_if_more_than)
            .with_keep_tags_if_version_less_than(self.keep_if_less_than);

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

        cleaner.prune_all_tags_and_clean().await?;
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
