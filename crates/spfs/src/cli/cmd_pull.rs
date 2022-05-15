// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;
use spfs::config::RemoteSpecifier;

#[macro_use]
mod args;

main!(CmdPull);

/// Pull one or more objects to the local repository
#[derive(Debug, Parser)]
pub struct CmdPull {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// The name or address of the remote server to pull from
    ///
    /// Defaults to searching all configured remotes
    #[clap(long, short)]
    remote: Option<String>,

    /// The reference(s) to pull/localize
    #[clap(value_name = "REF", required = true)]
    refs: Vec<String>,
}

impl CmdPull {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = config.get_repository().await?.into();
        let remote = match &self.remote {
            None => config.get_remote(RemoteSpecifier::Name("origin")).await?,
            Some(remote) => config.get_remote(RemoteSpecifier::Name(remote)).await?,
        };

        for reference in self.refs.iter() {
            spfs::sync_ref(reference, &remote, &repo).await?;
        }

        Ok(0)
    }
}
