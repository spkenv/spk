// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::PathBuf;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CmdWrite {
    #[structopt(
        long = "tag",
        short = "t",
        about = "Can be given many times: human-readable tags to update with the resulting object"
    )]
    tags: Vec<String>,
    #[structopt(
        long,
        short,
        about = "Write to a remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(
        long,
        short,
        about = "Store the contents of this file instead of reading from stdin"
    )]
    file: Option<PathBuf>,
}

impl CmdWrite {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };

        let reader: std::pin::Pin<Box<dyn tokio::io::AsyncRead + Sync + Send>> = match &self.file {
            Some(file) => Box::pin(tokio::fs::File::open(file).await?),
            None => Box::pin(tokio::io::stdin()),
        };

        let (payload, size) = repo.write_data(reader).await?;
        let blob = spfs::graph::Blob { payload, size };
        let digest = blob.digest();
        repo.write_blob(blob).await?;

        tracing::info!(?digest, "created");
        for tag in self.tags.iter() {
            let tag_spec = match spfs::tracking::TagSpec::parse(tag) {
                Ok(tag_spec) => tag_spec,
                Err(err) => {
                    tracing::warn!("cannot set invalid tag '{tag}': {err:?}");
                    continue;
                }
            };
            repo.push_tag(&tag_spec, &digest).await?;
            tracing::info!(?tag, "created");
        }

        Ok(0)
    }
}
