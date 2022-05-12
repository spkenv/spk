// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use futures::TryStreamExt;

/// List the current set of spfs runtimes
#[derive(Debug, Args)]
pub struct CmdRuntimes {
    /// Only print the name of each runtime, no additional data
    #[clap(short, long)]
    quiet: bool,
}

impl CmdRuntimes {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime_storage = config.get_runtime_storage().await?;
        let mut runtimes = runtime_storage.iter_runtimes().await;
        while let Some(runtime) = runtimes.try_next().await? {
            let mut message = runtime.name().to_string();
            if !self.quiet {
                message = format!(
                    "{message}\trunning={}\tpid={:?}\teditable={}",
                    runtime.is_running(),
                    runtime.get_pid(),
                    runtime.is_editable()
                )
            }
            println!("{message}");
        }
        Ok(0)
    }
}
