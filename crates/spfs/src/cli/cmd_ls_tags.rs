// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use tokio_stream::StreamExt;

/// List tags by their path
#[derive(Debug, Args)]
#[clap(visible_aliases = &["list-tags"])]
pub struct CmdLsTags {
    /// List tags from a remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// The tag path to list under
    #[clap(default_value = "/")]
    path: String,
}

impl CmdLsTags {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, &self.remote).await?;

        let path = relative_path::RelativePathBuf::from(&self.path);
        let mut names = repo.ls_tags(&path);

        // Accumulate the entries in order output them in sorted order.
        let mut entries = Vec::new();

        while let Some(item) = names.next().await {
            match item {
                Ok(name) => entries.push(name),
                Err(err) => tracing::error!("{err}"),
            }
        }

        for name in itertools::sorted(entries.iter()) {
            println!("{name}")
        }

        Ok(0)
    }
}
