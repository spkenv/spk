// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use chrono::prelude::*;
use clap::Args;
use colored::*;

use spfs::prelude::*;
use std::io::Write;

/// Clean the repository storage of any untracked data
///
/// Untracked data is any data that is not tagged or is not
/// attached to/used by a tagged object. This command also
/// provides semantics for pruning a repository from older
/// tag data to help detach data in the case of needing to
/// reduce repository size.
#[derive(Debug, Args)]
pub struct CmdClean {
    /// Trigger the clean operation on a remote repository
    #[clap(short, long)]
    remote: Option<String>,

    /// Don't prompt/ask before cleaning the data
    #[clap(long, short)]
    yes: bool,

    /// Prune tags older that the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s) (default: 9w)
    #[clap(long = "prune-if-older-than")]
    prune_if_older_than: Option<String>,

    /// Always keep tags newer than the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s) (default: 1w)
    #[clap(long = "keep-if-newer-than")]
    keep_if_newer_than: Option<String>,

    /// Prune tags if there are more than this number in a stream (default: 50)
    #[clap(long = "prune-if-more-than")]
    prune_if_more_than: Option<u64>,

    /// Always keep at least this number of tags in a stream (default: 10)
    #[clap(long = "keep-if-less-than")]
    keep_if_less_than: Option<u64>,
}

impl CmdClean {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };

        if self.prune_if_older_than.is_some()
            || self.keep_if_newer_than.is_some()
            || self.prune_if_more_than.is_some()
            || self.keep_if_less_than.is_some()
        {
            self.prune(&repo).await?;
        }

        let mut unattached = spfs::get_all_unattached_objects(&repo).await?;
        unattached.extend(spfs::get_all_unattached_payloads(&repo).await?);
        if unattached.is_empty() {
            tracing::info!("no objects to remove");
            return Ok(0);
        }
        tracing::info!("found {} objects to remove", unattached.len());
        if !self.yes {
            print!("  >--> Do you wish to proceed with the removal of these objects? [y/N]: ");
            let _ = std::io::stdout().flush();
            use std::io::BufRead;
            if let Some(line) = std::io::stdin().lock().lines().next() {
                let line = line?;
                if line != "y" {
                    return Ok(2);
                }
            }
        }

        match spfs::purge_objects(&unattached.iter().collect::<Vec<_>>(), &repo).await {
            Err(err) => Err(err),
            Ok(_) => {
                tracing::info!("clean successful");
                Ok(0)
            }
        }
    }

    async fn prune(&mut self, repo: &RepositoryHandle) -> spfs::Result<()> {
        let prune_if_older_than = age_to_date(
            self.prune_if_older_than
                .clone()
                .unwrap_or_else(|| "9w".into()),
        )?;
        let keep_if_newer_than = age_to_date(
            self.keep_if_newer_than
                .clone()
                .unwrap_or_else(|| "1w".into()),
        )?;
        let prune_if_more_than = self.prune_if_more_than.unwrap_or(50);
        let keep_if_less_than = self.keep_if_less_than.unwrap_or(10);

        let params = spfs::PruneParameters {
            prune_if_older_than: Some(prune_if_older_than),
            keep_if_newer_than: Some(keep_if_newer_than),
            prune_if_version_more_than: Some(prune_if_more_than),
            keep_if_version_less_than: Some(keep_if_less_than),
        };

        tracing::info!("collecting tags older than {:?}", prune_if_older_than);
        tracing::info!(
            "and collecting tags with version > {:?}",
            prune_if_more_than
        );
        tracing::info!("but leaving tags newer than {:?}", keep_if_newer_than);
        tracing::info!("and leaving tags with version <= {:?}", keep_if_less_than);

        tracing::info!("searching for tags to prune...");
        let to_prune = spfs::get_prunable_tags(repo, &params).await?;
        if to_prune.is_empty() {
            tracing::info!("no tags to prune");
            return Ok(());
        }

        for tag in to_prune.iter() {
            let spec = spfs::tracking::build_tag_spec(tag.org(), tag.name(), 0)?;
            let spec_str = spec.to_string(); // .ljust(tag.path().len() + 3);
            let mut info = tag.target.to_string();
            info.truncate(10);
            info = info.yellow().to_string();
            info += spec_str.bold().to_string().as_ref();
            info += tag.user.blue().to_string().as_ref();
            info += tag.time.to_string().blue().as_ref(); // %F %R
            println!("{info}");
        }

        if !self.yes {
            print!("  >--> Do you wish to proceed with the removal of these tag versions? [y/N]: ");
            let _ = std::io::stdout().flush();
            use std::io::BufRead;
            if let Some(line) = std::io::stdin().lock().lines().next() {
                let line = line?;
                if line != "y" {
                    return Err("Operation cancelled by user".into());
                }
            }
        }

        for tag in to_prune.iter() {
            repo.remove_tag(tag).await?;
        }
        Ok(())
    }
}

fn age_to_date(age: String) -> spfs::Result<DateTime<Utc>> {
    let (num, postfix) = age.split_at(age.len() - 1);
    let num: i64 = num
        .parse()
        .map_err(|err| spfs::Error::from(format!("{:?}", err)))?;
    if num < 0 {
        return Err(format!("provided age must be greater than zero: '{age}'").into());
    }

    match postfix {
        "y" => Ok(Utc::now() - chrono::Duration::weeks(num * 52)),
        "w" => Ok(Utc::now() - chrono::Duration::weeks(num)),
        "d" => Ok(Utc::now() - chrono::Duration::days(num)),
        "h" => Ok(Utc::now() - chrono::Duration::hours(num)),
        "m" => Ok(Utc::now() - chrono::Duration::minutes(num)),
        "s" => Ok(Utc::now() - chrono::Duration::seconds(num)),
        _ => {
            Err(format!("Unknown age postfix: '{postfix}', must be one of y, w, d, h, m, s").into())
        }
    }
}
