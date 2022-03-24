// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;
use structopt::StructOpt;
use tokio_stream::StreamExt;

use spfs::io::{self, DigestFormat};

#[derive(Debug, StructOpt)]
pub struct CmdTags {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Show layers from remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(long, about = "Also show the target digest of each tag")]
    target: bool,
    #[structopt(
        long,
        about = "Show the shortened form of each reported digest, implies --target"
    )]
    short: bool,
}

impl CmdTags {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };
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
