// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

#[macro_use]
mod args;

main!(CmdPull);

#[derive(Debug, StructOpt)]
#[structopt(about = "pull one or more objects to the local repository")]
pub struct CmdPull {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,
    #[structopt(
        long = "remote",
        short = "r",
        about = "the name or address of the remote server to pull from, \
                 defaults to searching all configured remotes"
    )]
    remote: Option<String>,
    #[structopt(
        value_name = "REF",
        required = true,
        about = "the reference(s) to pull/localize"
    )]
    refs: Vec<String>,
}

impl CmdPull {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let mut repo = config.get_repository()?.into();
        let remote = match &self.remote {
            None => config.get_remote("origin")?,
            Some(remote) => config.get_remote(remote)?,
        };

        for reference in self.refs.iter() {
            spfs::sync_ref(reference, &remote, &mut repo)?;
        }

        Ok(0)
    }
}
