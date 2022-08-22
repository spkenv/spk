// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;

/// Show the complete state of a runtime
#[derive(Debug, Args)]
pub struct CmdRuntimeInfo {
    /// Load a runtime in a remote or alternate repository
    #[clap(short, long)]
    remote: Option<String>,

    /// The name/id of the runtime to remove
    #[clap(env = "SPFS_RUNTIME")]
    name: String,
}

impl CmdRuntimeInfo {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime_storage = match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)
            }
            None => config.get_runtime_storage().await?,
        };

        let runtime = runtime_storage.read_runtime(&self.name).await?;
        serde_json::to_writer_pretty(std::io::stdout(), runtime.data())?;
        println!(); // the trailing new line is nice for interactive shells

        Ok(0)
    }
}
