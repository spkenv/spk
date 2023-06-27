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

    /// Mount the spfs filesystem in read-only mode (default if REF is non-empty)
    #[clap(long, overrides_with = "edit")]
    pub no_edit: bool,

    /// Name of a previously run durable runtime to run again.
    #[clap(long)]
    pub rerun: Option<String>,

    /// Provide a name for this runtime to make it easier to identify
    #[clap(long)]
    runtime_name: Option<String>,

    /// Use to keep the runtime around rather than deleting it when
    /// the process exits. This is best used with '--name NAME' to
    /// make rerunning the runtime easier at a later time.
    #[clap(short, long, env = "SPFS_KEEP_RUNTIME")]
    pub keep_runtime: bool,

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
            no_edit: self.no_edit,
            rerun: self.rerun.clone(),
            runtime_name: self.runtime_name.clone(),
            reference: self.reference.clone(),
            keep_runtime: self.keep_runtime,
            command: Default::default(),
            args: Default::default(),
        };
        run_cmd.run(config).await
    }
}
