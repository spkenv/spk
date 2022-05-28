// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use futures::TryStreamExt;

/// List runtime information from the repository
#[derive(Debug, Args)]
#[clap(visible_alias = "ls")]
pub struct CmdRuntimeList {
    /// List runtimes in a remote or alternate repository
    #[clap(short, long)]
    remote: Option<String>,

    /// Only print the name of each runtime, no additional data
    #[clap(short, long)]
    quiet: bool,
}

impl CmdRuntimeList {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime_storage = match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)
            }
            None => config.get_runtime_storage().await?,
        };

        let mut runtimes = runtime_storage.iter_runtimes().await;
        while let Some(runtime) = runtimes.try_next().await? {
            let mut message = runtime.name().to_string();
            if !self.quiet {
                message = format!(
                    "{message}\trunning={}\tpid={:?}\teditable={}",
                    runtime.status.running, runtime.status.owner, runtime.status.editable
                )
            }
            println!("{message}");
        }
        Ok(0)
    }
}
