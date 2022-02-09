// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

use spfs::{self, prelude::*};

#[derive(Debug, StructOpt)]
pub struct CmdTag {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Create tags in a remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(value_name = "TARGET_REF")]
    reference: String,
    #[structopt(value_name = "TAG", required = true)]
    tags: Vec<String>,
}

impl CmdTag {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };

        let target = repo.read_ref(self.reference.as_str()).await?.digest()?;
        for tag in self.tags.iter() {
            let tag = tag.parse()?;
            repo.push_tag(&tag, &target).await?;
            tracing::info!(?tag, "created");
        }
        Ok(0)
    }
}
