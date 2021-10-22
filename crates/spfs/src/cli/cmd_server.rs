// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

use spfs::prelude::*;

#[macro_use]
mod args;

main!(CmdServer);

#[derive(Debug, StructOpt)]
pub struct CmdServer {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,
    #[structopt(
        long = "remote",
        short = "r",
        about = "Serve a configured remote repository instead of the local one"
    )]
    remote: Option<String>,
}

impl CmdServer {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let mut _repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };

        tracing::info!("not implemented");
        Ok(1)
    }
}
