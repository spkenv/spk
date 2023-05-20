// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

use std::ffi::OsString;

use anyhow::{anyhow, bail, Context, Result};
use clap::{ArgGroup, Parser};
use futures::StreamExt;
use spfs::Error;
use spfs_cli_common as cli;
use spfs_cli_common::CommandName;

cli::main!(CmdJoin, sentry = false, sync = true);

/// Enter an existing runtime that is still active
#[derive(Parser, Debug)]
#[clap(group(
    ArgGroup::new("runtime_id")
    .required(true)
    .args(&["runtime", "pid"])))]
pub struct CmdJoin {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// The pid of a process in an active runtime, to join the same runtime
    #[clap(short, long)]
    pid: Option<u32>,

    /// The name or id of the runtime to join
    runtime: Option<String>,

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

impl CommandName for CmdJoin {
    fn command_name(&self) -> &'static str {
        "join"
    }
}

impl CmdJoin {
    pub fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        // because we are dealing with moving to a new linux namespace, we must
        // ensure that all code still operates in a single os thread
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| Error::process_spawn_error("new_current_thread()".into(), err, None))?;
        let spfs_runtime = rt.block_on(async {
            let storage = config.get_runtime_storage().await?;

            if let Some(runtime) = &self.runtime {
                storage
                    .read_runtime(runtime)
                    .await
                    .map_err(Into::<anyhow::Error>::into)
            } else if let Some(pid) = self.pid {
                let mount_ns = spfs::env::identify_mount_namespace_of_process(pid)
                    .await
                    .context("identify mount namespace of pid")?
                    .ok_or(anyhow!("pid not found"))?;
                let mut runtimes = storage.iter_runtimes().await;
                while let Some(runtime) = runtimes.next().await {
                    let Ok(runtime) = runtime else { continue; };
                    let Some(this_runtime_mount_ns) = &runtime.data().config.mount_namespace else { continue; };
                    if *this_runtime_mount_ns == mount_ns {
                        return Ok(runtime);
                    }
                }
                bail!("no runtime found for pid");
            } else {
                // Guaranteed by Clap config.
                unreachable!();
            }
        })?;

        // Shut down the tokio runtime (join threads) before attempting to
        // join the spfs runtime. This is only allowed in a single-threaded
        // program.

        // Do not block forever on drop because of any stuck blocking tasks.
        rt.shutdown_timeout(std::time::Duration::from_millis(250));

        spfs::env::join_runtime(&spfs_runtime)?;

        self.exec_runtime_command(&spfs_runtime)
    }

    fn exec_runtime_command(&mut self, rt: &spfs::runtime::Runtime) -> Result<i32> {
        let cmd = match self.command.take() {
            Some(exe) if !exe.is_empty() => {
                tracing::debug!("executing runtime command");
                spfs::build_shell_initialized_command(rt, None, exe, self.args.drain(..))?
            }
            _ => {
                tracing::debug!("starting interactive shell environment");
                spfs::build_interactive_shell_command(rt, None)?
            }
        };
        let mut proc = cmd.into_std();
        tracing::debug!("{:?}", proc);
        Ok(proc
            .status()
            .map_err(|err| Error::process_spawn_error("exec_runtime_command".into(), err, None))?
            .code()
            .unwrap_or(1))
    }
}
