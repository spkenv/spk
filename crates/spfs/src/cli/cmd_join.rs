// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;
use spfs::Result;
use std::ffi::OsString;

#[macro_use]
mod args;

main!(CmdJoin, sentry = false, sync = true);

/// Enter an existing runtime that is still active
#[derive(Parser, Debug)]
pub struct CmdJoin {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// The name or id of the runtime to join
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

impl CmdJoin {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        // because we are dealing with moving to a new linux namespace, we must
        // ensure that all code still operates in a single os thread
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let res = rt.block_on(async {
            let storage = config.get_runtime_storage().await?;
            let rt = storage.read_runtime(&self.runtime).await?;
            spfs::env::join_runtime(&rt)?;

            self.exec_runtime_command(&rt).await
        });
        // do not block forever on drop because of any stuck blocking tasks
        rt.shutdown_timeout(std::time::Duration::from_millis(250));
        res
    }

    async fn exec_runtime_command(&mut self, rt: &spfs::runtime::Runtime) -> Result<i32> {
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
        let mut proc = cmd.into_std();
        tracing::debug!("{:?}", proc);
        Ok(proc.status()?.code().unwrap_or(1))
    }
}
