// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;
use tokio_stream::StreamExt;

use spfs::io::{self, DigestFormat};

#[derive(Debug, StructOpt)]
pub struct CmdPlatforms {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Show layers from remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(long, about = "Show the shortened form of each reported layer digest")]
    short: bool,
    #[structopt(
        long,
        about = "Also find and report any tags that point to this object, implies --short"
    )]
    tags: bool,
}

impl CmdPlatforms {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };
        let mut platforms = repo.iter_platforms();
        while let Some(platform) = platforms.next().await {
            let (digest, _) = platform?;
            println!("{}", self.format_digest(digest, &repo).await?);
        }
        Ok(0)
    }

    async fn format_digest<'repo>(
        &self,
        digest: spfs::encoding::Digest,
        repo: &'repo spfs::storage::RepositoryHandle,
    ) -> spfs::Result<String> {
        if self.tags {
            io::format_digest(digest, DigestFormat::ShortenedWithTags(repo)).await
        } else if self.short {
            io::format_digest(digest, DigestFormat::Shortened(repo)).await
        } else {
            io::format_digest(digest, DigestFormat::Full).await
        }
    }
}
