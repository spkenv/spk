// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::{Context, IntoDiagnostic, Result};
use spfs_cli_common as cli;

/// Show the complete state of a runtime
#[derive(Debug, Args)]
pub struct CmdRuntimeInfo {
    /// Load a runtime in a remote or alternate repository
    #[clap(short, long)]
    remote: Option<String>,

    #[clap(flatten)]
    annotation: cli::AnnotationViewing,

    /// The name/id of the runtime to remove
    #[clap(env = "SPFS_RUNTIME")]
    name: String,
}

impl CmdRuntimeInfo {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let runtime_storage = match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)?
            }
            None => config.get_runtime_storage().await?,
        };

        let runtime = runtime_storage.read_runtime(&self.name).await?;

        if self.annotation.get_all || self.annotation.get.is_some() {
            self.annotation.print_data(&runtime).await?;
            return Ok(0);
        }

        serde_json::to_writer_pretty(std::io::stdout(), runtime.data())
            .into_diagnostic()
            .wrap_err("Failed to generate json output")?;
        println!(); // the trailing new line is nice for interactive shells

        Ok(0)
    }
}
