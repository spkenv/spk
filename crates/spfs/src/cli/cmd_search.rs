// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;
use tokio_stream::StreamExt;

#[derive(Debug, StructOpt)]
pub struct CmdSearch {
    #[structopt(value_name = "TERM", about = "The search term/substring to look for")]
    term: String,
}

impl CmdSearch {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let mut repos = Vec::with_capacity(config.remote.len());
        for name in config.list_remote_names() {
            let remote = match config.get_remote(&name) {
                Ok(remote) => remote,
                Err(err) => {
                    tracing::warn!(remote = %name, "failed to load remote repository");
                    tracing::debug!(" > {:?}", err);
                    continue;
                }
            };
            repos.push(remote);
        }
        repos.insert(0, config.get_repository()?.into());
        for repo in repos.into_iter() {
            let mut tag_streams = repo.iter_tags();
            while let Some(tag) = tag_streams.next().await {
                let (tag, _) = tag?;
                if tag.to_string().contains(&self.term) {
                    println!("{:?}", tag);
                }
            }
        }
        Ok(0)
    }
}
