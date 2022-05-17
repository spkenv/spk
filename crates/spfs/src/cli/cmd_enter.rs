// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;

use clap::Parser;

#[macro_use]
mod args;

// The runtime setup process manages the current namespace
// which operates only on the current thread. For this reason
// we must use a single threaded async runtime, if any.
main!(CmdEnter, sentry = false, sync = true);

/// Run a command in a configured spfs runtime
#[derive(Debug, Parser)]
#[clap(name = "spfs-enter")]
pub struct CmdEnter {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Remount the overlay filesystem, don't enter a new namepace
    #[clap(short, long)]
    remount: bool,

    /// The address of the storage being used for runtimes
    #[clap(long)]
    runtime_storage: url::Url,

    /// The name of the runtime being entered
    #[clap(long)]
    runtime: String,

    /// The command to run after initialization
    #[clap(required = true)]
    cmd: Vec<OsString>,
}

impl CmdEnter {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        // we need a single-threaded runtime in order to properly setup
        // and enter the namespace of the runtime
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                spfs::Error::String(format!("Failed to establish async runtime: {:?}", err))
            })?;
        rt.block_on(self.run_async(config))
    }

    pub async fn run_async(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = spfs::open_repository(&self.runtime_storage).await?;
        let storage = spfs::runtime::Storage::new(repo);
        let runtime = storage.read_runtime(&self.runtime).await?;
        if self.remount {
            spfs::reinitialize_runtime(&runtime).await?;
            Ok(0)
        } else {
            let owned = spfs::runtime::OwnedRuntime::upgrade(runtime).await?;
            tracing::debug!("initalizing runtime");
            spfs::initialize_runtime(&owned, config).await?;

            owned.ensure_startup_scripts()?;
            std::env::set_var("SPFS_RUNTIME", owned.name());

            let res = self.exec_runtime_command(&owned);
            if let Err(err) = owned.delete().await {
                tracing::error!("failed to clean up runtime data: {err:?}")
            }
            res
        }
    }

    fn exec_runtime_command(&mut self, rt: &spfs::runtime::OwnedRuntime) -> spfs::Result<i32> {
        let mut cmd: Vec<_> = self.cmd.drain(..).collect();
        if cmd.is_empty() || cmd[0] == *"" {
            cmd = spfs::build_interactive_shell_cmd(rt)?;
            tracing::debug!("starting interactive shell environment");
        } else {
            cmd =
                spfs::build_shell_initialized_command(rt, cmd[0].clone(), &mut cmd[1..].to_vec())?;
            tracing::debug!("executing runtime command");
        }
        tracing::debug!(?cmd);
        let mut proc = std::process::Command::new(cmd[0].clone());
        proc.args(&cmd[1..]);
        tracing::debug!("{:?}", proc);
        Ok(proc.status()?.code().unwrap_or(1))
    }
}
