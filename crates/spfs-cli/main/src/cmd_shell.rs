// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;
use spfs_cli_common as cli;

use super::cmd_run;

/// Enter a subshell in a configured spfs environment
#[derive(Debug, Args)]
pub struct CmdShell {
    #[clap(flatten)]
    sync: cli::Sync,

    #[clap(flatten)]
    logging: cli::Logging,

    /// Mount the spfs filesystem in edit mode (true if REF is empty or not given)
    #[clap(short, long)]
    edit: bool,

    /// Provide a name for this runtime to make it easier to identify
    #[clap(short, long)]
    name: Option<String>,

    /// The tag or id of the desired runtime
    ///
    /// Use '-' or nothing to request an empty environment
    #[clap(name = "REF")]
    reference: spfs::tracking::EnvSpec,
}

impl CmdShell {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let mut run_cmd = cmd_run::CmdRun {
            sync: self.sync.clone(),
            logging: self.logging.clone(),
            edit: self.edit,
            name: self.name.clone(),
            reference: self.reference.clone(),
            command: Default::default(),
            args: Default::default(),
        };
        run_cmd.run(config).await
    }
}
