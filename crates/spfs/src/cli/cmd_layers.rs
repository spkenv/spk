// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use tokio_stream::StreamExt;

use spfs::io::{self, DigestFormat};

/// List all layers in an spfs repository
#[derive(Debug, Args)]
pub struct CmdLayers {
    /// Show layers from remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// Show the shortened form of each reported layer digest
    #[clap(long)]
    short: bool,

    /// Also find and report any tags that point to each layer, implies --short
    #[clap(long)]
    tags: bool,
}

impl CmdLayers {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = config.get_remote_repository_or_local(&self.remote).await?;

        let mut layers = repo.iter_layers();
        while let Some(layer) = layers.next().await {
            let (digest, _) = layer?;
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
