// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;

use anyhow::{Context, Result};
use clap::Args;
use spfs_cli_common as cli;

/// Run a program in a configured spfs environment
#[derive(Debug, Args)]
pub struct CmdRun {
    #[clap(flatten)]
    pub sync: cli::Sync,

    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Mount the spfs filesystem in edit mode (true if REF is empty or not given)
    #[clap(short, long)]
    pub edit: bool,

    /// Provide a name for this runtime to make it easier to identify
    #[clap(short, long)]
    pub name: Option<String>,

    /// The tag or id of the desired runtime
    ///
    /// Use '-' or an empty string to request an empty environment
    pub reference: spfs::tracking::EnvSpec,

    /// The command to run in the environment
    pub command: OsString,

    /// Additional arguments to provide to the command
    ///
    /// In order to ensure that flags are passed as-is, place '--' before
    /// specifying any flags that should be given to the subcommand:
    ///   eg `spfs enter <args> -- command --flag-for-command`
    pub args: Vec<OsString>,
}

impl CmdRun {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let (repo, runtimes) = tokio::try_join!(
            config.get_local_repository_handle(),
            config.get_runtime_storage()
        )?;
        let mut runtime = match &self.name {
            Some(name) => runtimes.create_named_runtime(name).await?,
            None => runtimes.create_runtime().await?,
        };
        if self.reference.is_empty() {
            self.edit = true;
        } else {
            let origin = config.get_remote("origin").await?;
            let synced = self
                .sync
                .get_syncer(&origin, &repo)
                .sync_env(self.reference.clone())
                .await?;
            for item in synced.env.iter() {
                let digest = item.resolve_digest(&*repo).await?;
                runtime.push_digest(digest);
            }
        }

        runtime.status.command = vec![self.command.to_string_lossy().to_string()];
        runtime
            .status
            .command
            .extend(self.args.iter().map(|s| s.to_string_lossy().to_string()));
        runtime.status.editable = self.edit;
        runtime.save_state_to_storage().await?;

        tracing::debug!("resolving entry process");
        let cmd = spfs::build_command_for_runtime(&runtime, &self.command, self.args.drain(..))?;
        cmd.exec()
            .map(|_| 0)
            .context("Failed to execute runtime command")
    }
}
