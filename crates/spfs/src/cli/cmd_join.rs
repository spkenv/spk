// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spfs::Result;
use std::ffi::OsString;
use structopt::StructOpt;

#[macro_use]
mod args;

main!(CmdJoin, sentry = false);

#[derive(StructOpt, Debug)]
#[structopt(about = "enter an existing runtime that is still active")]
pub struct CmdJoin {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,
    #[structopt(about = "The name or id of the runtime to join")]
    runtime: String,
    #[structopt(about = "Optional command to run in the environment, spawns a shell if not given")]
    cmd: Vec<OsString>,
}

impl CmdJoin {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let storage = config.get_runtime_storage()?;
        let rt = storage.read_runtime(&self.runtime)?;
        spfs::env::join_runtime(&rt)?;

        exec_runtime_command(self.cmd.clone())
    }
}

fn exec_runtime_command(mut cmd: Vec<OsString>) -> Result<i32> {
    if cmd.is_empty() || cmd[0] == *"" {
        cmd = spfs::build_interactive_shell_cmd(&spfs::active_runtime()?)?;
        tracing::debug!("starting interactive shell environment");
    } else {
        cmd = spfs::build_shell_initialized_command(cmd[0].clone(), &mut cmd[1..].to_vec())?;
        tracing::debug!("executing runtime command");
    }
    tracing::debug!(?cmd);
    let mut proc = std::process::Command::new(cmd[0].clone());
    proc.args(&cmd[1..]);
    tracing::debug!("{:?}", proc);
    Ok(proc.status()?.code().unwrap_or(1))
}
