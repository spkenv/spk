// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use colored::Colorize;
use tokio_stream::StreamExt;

use spfs::io::{self, DigestFormat};

/// List all tags in an spfs repository
#[derive(Debug, Args)]
pub struct CmdTags {
    /// Show layers from remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// Also show the target digest of each tag
    #[clap(long)]
    target: bool,

    /// Show the shortened form of each reported digest, implies --target
    #[clap(long)]
    short: bool,
}

impl CmdTags {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let mut tag_streams = repo.iter_tags();
        while let Some((tag_spec, tag)) = tag_streams.try_next().await? {
            let suffix = if self.short {
                format!(
                    " {} {}",
                    "->".cyan(),
                    io::format_digest(tag.target, DigestFormat::Shortened(&repo))
                        .await?
                        .dimmed()
                )
            } else if self.target {
                format!(
                    " {} {}",
                    "->".cyan(),
                    io::format_digest(tag.target, DigestFormat::Full)
                        .await?
                        .dimmed()
                )
            } else {
                String::new()
            };
            println!("{}{}", tag_spec, suffix);
        }
        Ok(0)
    }
}
