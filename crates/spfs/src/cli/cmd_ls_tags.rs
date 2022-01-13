// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;
use tokio_stream::StreamExt;

#[derive(Debug, StructOpt)]
pub struct CmdLsTags {
    #[structopt(
        long = "remote",
        short = "r",
        about = "List tags from a remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(
        default_value = "/",
        about = "The tag path to list under, defaults to the root ('/')"
    )]
    path: String,
}

impl CmdLsTags {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };

        let path = relative_path::RelativePathBuf::from(&self.path);
        let mut names = repo.ls_tags(&path);
        while let Some(item) = names.next().await {
            match item {
                Ok(name) => println!("{}", name),
                Err(err) => tracing::error!("{}", err),
            }
        }
        Ok(0)
    }
}
