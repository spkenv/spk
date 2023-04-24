// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

use std::ffi::{CString, OsString};
use std::os::unix::prelude::OsStringExt;

use anyhow::{Context, Result};
use clap::Parser;
use spfs::env::SPFS_MONITOR_FOREGROUND_LOGGING_VAR;
use spfs_cli_common as cli;
use spfs_cli_common::CommandName;
use tokio::io::AsyncWriteExt;

// The runtime setup process manages the current namespace
// which operates only on the current thread. For this reason
// we must use a single threaded async runtime, if any.
cli::main!(CmdEnter, sentry = false, sync = true);

/// Run a command in a configured spfs runtime
#[derive(Debug, Parser)]
#[clap(name = "spfs-enter")]
pub struct CmdEnter {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Remount the overlay filesystem, don't enter a new namespace
    #[clap(short, long)]
    remount: bool,

    /// The address of the storage being used for runtimes
    ///
    /// Defaults to the current configured local repository.
    #[clap(long)]
    runtime_storage: Option<url::Url>,

    /// The value to set $TMPDIR to in new environment
    #[clap(long)]
    tmpdir: Option<String>,

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
    ///   eg `spfs enter <args> -- command --flag-for-command`
    args: Vec<OsString>,
}

impl CommandName for CmdEnter {
    fn command_name(&self) -> &'static str {
        "enter"
    }
}

impl CmdEnter {
    pub fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        // we need a single-threaded runtime in order to properly setup
        // and enter the namespace of the runtime
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                spfs::Error::String(format!("Failed to establish async runtime: {err:?}"))
            })?;
        let owned_runtime = rt.block_on(self.setup_runtime(config))?;
        // do not block forever on drop because of any stuck blocking tasks
        rt.shutdown_timeout(std::time::Duration::from_millis(250));
        if let Some(rt) = owned_runtime {
            self.exec_runtime_command(rt)
        } else {
            Ok(0)
        }
    }

    pub async fn setup_runtime(
        &mut self,
        config: &spfs::Config,
    ) -> Result<Option<spfs::runtime::OwnedRuntime>> {
        let mut runtime = self.load_runtime(config).await?;
        if self.remount {
            spfs::reinitialize_runtime(&mut runtime).await?;
            Ok(None)
        } else {
            let mut owned = spfs::runtime::OwnedRuntime::upgrade_as_owner(runtime).await?;

            // At this point, our pid is owned by root and has not moved into
            // the proper mount namespace; spfs-monitor will not be able to
            // read the namespace because it runs as a normal user, and would
            // read the incorrect namespace if it were to read it right now.

            let mut monitor_stdin = match spfs::env::spawn_monitor_for_runtime(&owned) {
                Err(err) => {
                    if let Err(err) = owned.delete().await {
                        tracing::error!(
                            ?err,
                            "failed to cleanup runtime data after failure to start monitor"
                        );
                    }
                    return Err(err.into());
                }
                Ok(mut child) => child.stdin.take().ok_or_else(|| {
                    spfs::Error::from("monitor was spawned without stdin attached")
                })?,
            };

            tracing::debug!("initializing runtime");
            spfs::initialize_runtime(&mut owned).await?;

            // We promise to not mutate the runtime after this point.
            // spfs-monitor will read and modify it after we tell it to
            // proceed.
            let owned = owned;

            // Now we have dropped privileges and are running as the invoking
            // user (same uid as spfs-monitor) and have entered the mount
            // namespace that spfs-monitor should be monitoring. Inform it to
            // proceed.
            tracing::debug!("informing spfs-monitor to proceed");
            let send_go = async move {
                monitor_stdin.write_all("go".as_bytes()).await?;
                monitor_stdin.flush().await?;
                Ok::<_, std::io::Error>(())
            }
            .await;
            match send_go {
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
                    // Pipe error generally means the spfs-monitor process is
                    // gone. If it failed to start/quit prematurely then it
                    // may have already deleted the runtime. It is not safe to
                    // proceed with using the runtime, so we don't ignore this
                    // error and hope for the best. Using this new environment
                    // puts whatever is using it at risk of data loss.
                    anyhow::bail!(
                        "spfs-monitor disappeared unexpectedly, it is unsafe to continue. Setting ${SPFS_MONITOR_FOREGROUND_LOGGING_VAR}=1 may provide more details"
                    );
                }
                Err(err) => {
                    anyhow::bail!("Failed to inform spfs-monitor to start: {err}");
                }
            };

            owned.ensure_startup_scripts(&self.tmpdir)?;
            std::env::set_var("SPFS_RUNTIME", owned.name());

            Ok(Some(owned))
        }
    }

    async fn load_runtime(&self, config: &spfs::Config) -> Result<spfs::runtime::Runtime> {
        let repo = match &self.runtime_storage {
            Some(address) => spfs::open_repository(address).await?,
            None => config.get_local_repository_handle().await?,
        };
        let storage = spfs::runtime::Storage::new(repo);
        storage
            .read_runtime(&self.runtime)
            .await
            .map_err(|err| err.into())
    }

    fn exec_runtime_command(&mut self, rt: spfs::runtime::OwnedRuntime) -> Result<i32> {
        let cmd = match self.command.take() {
            Some(exe) if !exe.is_empty() => {
                tracing::debug!("executing runtime command");
                spfs::build_shell_initialized_command(&rt, None, exe, self.args.drain(..))?
            }
            _ => {
                tracing::debug!("starting interactive shell environment");
                spfs::build_interactive_shell_command(&rt, None)?
            }
        };

        tracing::debug!("{cmd:?}");
        let exe = CString::new(cmd.executable.into_vec())
            .context("Executable name cannot contain a nul character")?;
        let mut argv = vec![exe];
        argv.extend(
            cmd.args
                .into_iter()
                .map(OsStringExt::into_vec)
                .map(CString::new)
                .map(|res| res.context("Command arguments cannot contain a nul character"))
                .collect::<Result<Vec<_>>>()?,
        );
        nix::unistd::execvp(&argv[0], &argv).context("Failed to launch subcommand")?;
        unreachable!("execvp does not return")
    }
}
