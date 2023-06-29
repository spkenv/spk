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
    #[clap(flatten)]
    pub logging: cli::Logging,

    /// The pid of a process in an active runtime, to join the same runtime
    #[clap(short, long)]
    pid: Option<u32>,

    /// The name or id of the runtime to join
    runtime: Option<String>,

    /// The command and extra arguments to run after initialization
    ///
    /// If not given, run an interactive shell environment. If given, it must
    /// be separated from the other arguments with `--`.
    #[arg(last = true)]
    command: Vec<OsString>,
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
                let mount_ns = spfs::monitor::identify_mount_namespace_of_process(pid)
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

        // Wait as long as it takes to shutdown the Tokio runtime. We can't
        // proceed with `join_runtime` until all background threads are
        // terminated.
        drop(rt);

        let mut try_counter = 0;
        const TIME_TO_WAIT_BETWEEN_ATTEMPTS: std::time::Duration =
            std::time::Duration::from_millis(10);
        debug_assert!(TIME_TO_WAIT_BETWEEN_ATTEMPTS.as_millis() < 500);
        const ATTEMPTS_PER_SECOND: u128 = 1000u128 / TIME_TO_WAIT_BETWEEN_ATTEMPTS.as_millis();
        loop {
            try_counter += 1;
            match spfs::env::RuntimeConfigurator::default().join_runtime(&spfs_runtime) {
                Err(spfs::Error::String(err)) if err.contains("single-threaded") => {
                    // Anecdotally it takes one retry to succeed; don't start
                    // to log anything until it is taking longer than usual.
                    // Don't log every attempt since this retries rapidly.
                    if try_counter % (ATTEMPTS_PER_SECOND / 2) == 0 {
                        tracing::info!("Waiting for process to become single threaded: {err}");
                    }
                    std::thread::sleep(TIME_TO_WAIT_BETWEEN_ATTEMPTS);
                }
                Err(err) => return Err(err.into()),
                Ok(_) => break,
            }
        }

        self.exec_runtime_command(&spfs_runtime)
    }

    fn exec_runtime_command(&mut self, rt: &spfs::runtime::Runtime) -> Result<i32> {
        let cmd = match self.command.first() {
            Some(exe) if !exe.is_empty() => {
                tracing::debug!("executing runtime command");
                spfs::build_shell_initialized_command(rt, None, exe, self.command.iter().skip(1))?
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
