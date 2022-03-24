// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;

#[macro_use]
mod args;

main!(CmdPush);

/// Push one or more objects to a remote repository
#[derive(Debug, Parser)]
pub struct CmdPush {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// The name or address of the remote server to push to
    #[clap(long, short, default_value = "origin")]
    remote: String,

    /// The reference(s) to push
    #[clap(value_name = "REF", required = true)]
    refs: Vec<String>,
}

impl CmdPush {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = config.get_repository().await?.into();
        let remote = config.get_remote(&self.remote).await?;
        for reference in self.refs.iter() {
            spfs::sync_ref(reference, &repo, &remote).await?;
        }

        Ok(0)
    }
}
