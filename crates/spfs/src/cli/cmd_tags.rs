// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;
use tokio_stream::StreamExt;

#[derive(Debug, StructOpt)]
pub struct CmdTags {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Show layers from remote repository instead of the local one"
    )]
    remote: Option<String>,
}

impl CmdTags {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository()?.into(),
        };
        let mut tag_streams = repo.iter_tags();
        while let Some(tag) = tag_streams.next().await {
            let (_, tag) = tag?;
            println!(
                "{}",
                spfs::io::format_digest(&tag.target.to_string(), Some(&repo)).await?
            );
        }
        Ok(0)
    }
}
