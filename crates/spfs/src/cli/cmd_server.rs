// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use structopt::StructOpt;

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
    // 7737 = spfs on a dial pad
    #[structopt(default_value = "0.0.0.0:7737", about = "The address to listen on")]
    address: std::net::SocketAddr,
}

impl CmdServer {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };

        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async move { spfs::server::run(self.address, repo).await })?;
        Ok(1)
    }
}
