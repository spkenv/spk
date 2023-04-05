// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;
use tokio_stream::StreamExt;

/// Search for available tags by substring
#[derive(Debug, Args)]
pub struct CmdSearch {
    /// The search term/substring to look for
    #[clap(value_name = "TERM")]
    term: String,
}

impl CmdSearch {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let mut repos = Vec::with_capacity(config.remote.len());
        for name in config.list_remote_names() {
            let remote = match config.get_remote(&name).await {
                Ok(remote) => remote,
                Err(err) => {
                    tracing::warn!(remote = %name, "failed to load remote repository");
                    tracing::debug!(" > {:?}", err);
                    continue;
                }
            };
            repos.push(remote);
        }
        repos.insert(0, config.get_local_repository().await?.into());
        for repo in repos.into_iter() {
            let mut tag_streams = repo.iter_tags();
            while let Some(tag) = tag_streams.next().await {
                let (tag, _) = tag?;
                if tag.to_string().contains(&self.term) {
                    println!("{tag:?}");
                }
            }
        }
        Ok(0)
    }
}
