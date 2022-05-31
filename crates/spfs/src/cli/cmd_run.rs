// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;

use clap::Parser;

use spfs::prelude::*;

/// Run a program in a configured spfs environment
#[derive(Debug, Parser)]
#[clap(name = "spfs-run")]
pub struct CmdRun {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Try to pull the latest iteration of each tag even if it exists locally
    #[clap(short, long)]
    pub pull: bool,

    /// Mount the spfs filesystem in edit mode (true if REF is empty or not given)
    #[clap(short, long)]
    pub edit: bool,

    /// Provide a name for this runtime to make it easier to identify
    #[clap(short, long)]
    pub name: Option<String>,

    /// The tag or id of the desired runtime
    ///
    /// Use '-' or an empty string to request an empty environment
    pub reference: String,

    /// The command to run in the environment
    pub command: OsString,

    /// Additional arguments to provide to the command
    ///
    /// In order to ensure that flags are passed as-is, place '--' before
    /// specifying any flags that should be given to the subcommand:
    ///   eg spfs enter <args> -- command --flag-for-command
    pub args: Vec<OsString>,
}

impl CmdRun {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = config.get_local_repository_handle().await?;
        let runtimes = config.get_runtime_storage().await?;
        let mut runtime = match &self.name {
            Some(name) => runtimes.create_named_runtime(name).await?,
            None => runtimes.create_runtime().await?,
        };
        match self.reference.as_str() {
            "-" | "" => self.edit = true,
            reference => {
                let env_spec = spfs::tracking::EnvSpec::parse(reference)?;
                for target in env_spec.iter() {
                    let target = target.to_string();
                    if self.pull || !repo.has_ref(target.as_str()).await {
                        tracing::info!(reference = ?target, "pulling target ref");
                        let origin = config.get_remote("origin").await?;
                        spfs::Syncer::new(&origin, &repo)
                            .sync_ref(target.as_str())
                            .await?;
                    }

                    let obj = repo.read_ref(target.as_str()).await?;
                    runtime.push_digest(&obj.digest()?);
                }
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
        tracing::trace!(?cmd);
        let exe = std::ffi::CString::new(cmd.executable.as_bytes()).unwrap();
        let mut args: Vec<_> = cmd
            .args
            .into_iter()
            .map(|arg| std::ffi::CString::new(arg.as_bytes()).unwrap())
            .collect();
        args.insert(0, exe.clone());
        nix::unistd::execv(exe.as_ref(), args.as_slice())?;
        Ok(0)
    }
}
