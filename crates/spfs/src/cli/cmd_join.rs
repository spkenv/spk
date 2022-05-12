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

    /// Optional command to run in the environment, spawns a shell if not given
    cmd: Vec<OsString>,
}

impl CmdJoin {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        // because we are dealing with moveing to a new linux namespace, we must
        // ensure that all code still operates in a single os thread
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            let storage = config.get_runtime_storage().await?;
            let rt = storage.read_runtime(&self.runtime).await?;
            spfs::env::join_runtime(&rt)?;

            exec_runtime_command(self.cmd.clone()).await
        })
    }
}

async fn exec_runtime_command(mut cmd: Vec<OsString>) -> Result<i32> {
    let rt = spfs::active_runtime().await?;
    if cmd.is_empty() || cmd[0] == *"" {
        cmd = spfs::build_interactive_shell_cmd(&rt)?;
        tracing::debug!("starting interactive shell environment");
    } else {
        cmd = spfs::build_shell_initialized_command(&rt, cmd[0].clone(), &mut cmd[1..].to_vec())?;
        tracing::debug!("executing runtime command");
    }
    tracing::debug!(?cmd);
    let mut proc = std::process::Command::new(cmd[0].clone());
    proc.args(&cmd[1..]);
    tracing::debug!("{:?}", proc);
    Ok(proc.status()?.code().unwrap_or(1))
}
