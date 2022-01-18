// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

#[macro_use]
mod args;

main!(CmdPush);

#[derive(Debug, StructOpt)]
#[structopt(about = "push one or more objects to a remote repository")]
pub struct CmdPush {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,
    #[structopt(
        long = "remote",
        short = "r",
        default_value = "origin",
        about = "the name or address of the remote server to push to"
    )]
    remote: String,
    #[structopt(
        value_name = "REF",
        required = true,
        about = "the reference(s) to push"
    )]
    refs: Vec<String>,
}

impl CmdPush {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = config.get_repository()?.into();
        let mut remote = config.get_remote(&self.remote).await?;
        for reference in self.refs.iter() {
            spfs::sync_ref(reference, &repo, &mut remote).await?;
        }

        Ok(0)
    }
}
