// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spfs::io::{self, DigestFormat};
use spfs::prelude::*;
use tokio_stream::StreamExt;

/// List all platforms in an spfs repository
#[derive(Debug, Args)]
pub struct CmdPlatforms {
    /// Show layers from remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// Show the shortened form of each reported layer digest
    #[clap(long)]
    short: bool,

    /// Also find and report any tags that point to each platform, implies --short
    #[clap(long)]
    tags: bool,
}

impl CmdPlatforms {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let mut platforms = repo.iter_platforms();
        while let Some(platform) = platforms.next().await {
            let (digest, _) = platform?;
            println!("{}", self.format_digest(digest, &repo).await?);
        }
        Ok(0)
    }

    async fn format_digest(
        &self,
        digest: spfs::encoding::Digest,
        repo: &spfs::storage::RepositoryHandle,
    ) -> Result<String> {
        if self.tags {
            io::format_digest(digest, DigestFormat::ShortenedWithTags(repo)).await
        } else if self.short {
            io::format_digest(digest, DigestFormat::Shortened(repo)).await
        } else {
            io::format_digest(digest, DigestFormat::Full).await
        }
        .map_err(|err| err.into())
    }
}
