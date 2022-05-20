// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;

use clap::Parser;
use tokio::signal::unix::{signal, SignalKind};

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
    ///
    /// If not given, run an interactive shell environment
    command: Option<OsString>,

    /// Additional arguments to provide to the command
    ///
    /// In order to ensure that flags are passed as-is, place '--' before
    /// specifying any flags that should be given to the subcommand:
    ///   eg spfs enter <args> -- command --flag-for-command
    args: Vec<OsString>,
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
            let mut terminate = signal(SignalKind::terminate())?;
            let mut interrupt = signal(SignalKind::interrupt())?;
            let mut quit = signal(SignalKind::quit())?;
            let owned = spfs::runtime::OwnedRuntime::upgrade_as_owner(runtime).await?;

            if let Err(err) = spfs::env::spawn_monitor_for_runtime(&owned) {
                if let Err(err) = owned.delete().await {
                    tracing::error!(
                        ?err,
                        "failed to cleanup runtime data after failure to start monitor"
                    );
                }
                return Err(err);
            }

            tracing::debug!("initalizing runtime");
            spfs::initialize_runtime(&owned, config).await?;

            owned.ensure_startup_scripts()?;
            std::env::set_var("SPFS_RUNTIME", owned.name());

            let mut child = self.exec_runtime_command(&owned).await?;
            let res = loop {
                tokio::select! {
                    res = child.wait() => break res,
                    // we explicitly catch and ignore signals related to interruption
                    // assuming that the child process will receive them and act
                    // accordingly. This is also to ensure that we never exit before
                    // the child and forget to clean up the runtime data
                    _ = terminate.recv() => {},
                    _ = interrupt.recv() => {},
                    _ = quit.recv() => {},
                }
            };

            Ok(res?.code().unwrap_or(1))
        }
    }

    async fn exec_runtime_command(
        &mut self,
        rt: &spfs::runtime::OwnedRuntime,
    ) -> spfs::Result<tokio::process::Child> {
        let cmd = match self.command.take() {
            Some(exe) if !exe.is_empty() => {
                tracing::debug!("executing runtime command");
                spfs::build_shell_initialized_command(rt, exe, self.args.drain(..))?
            }
            _ => {
                tracing::debug!("starting interactive shell environment");
                spfs::build_interactive_shell_command(rt)?
            }
        };
        let mut proc = cmd.into_tokio();
        tracing::debug!("{:?}", proc);
        Ok(proc.spawn()?)
    }
}
