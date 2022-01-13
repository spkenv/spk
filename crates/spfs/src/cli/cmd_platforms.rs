// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use futures::stream::StreamExt;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CmdPlatforms {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Show layers from remote repository instead of the local one"
    )]
    remote: Option<String>,
}

impl CmdPlatforms {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };
        let mut platforms = repo.iter_platforms();
        while let Some(platform) = platforms.next().await {
            let (digest, _) = platform?;
            println!(
                "{}",
                spfs::io::format_digest(&digest.to_string(), Some(&repo)).await?
            );
        }
        Ok(0)
    }
}
