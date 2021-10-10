// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CmdUntag {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Remove tags in a remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(value_name = "TAG", required = true, help = "The tag(s) to remove")]
    tags: Vec<String>,
}

impl CmdUntag {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let mut repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };

        for tag in self.tags.iter() {
            let tag = tag.parse()?;
            repo.remove_tag_stream(&tag)?;
            tracing::info!(?tag, "removed");
        }
        Ok(0)
    }
}
