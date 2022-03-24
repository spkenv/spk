// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;
use spfs::Result;
use std::ffi::OsString;

#[macro_use]
mod args;

main!(CmdInit);

#[derive(Parser, Debug)]
pub struct CmdInit {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// The root directory of the runtime being initialized
    #[clap(long = "runtime-dir")]
    runtime_root_dir: Option<String>,

    /// The command to run after initialization
    #[clap(required = true)]
    cmd: Vec<OsString>,
}

impl CmdInit {
    pub async fn run(&mut self, _config: &spfs::Config) -> spfs::Result<i32> {
        tracing::debug!("initializing runtime environment");
        let _handle = match &self.runtime_root_dir {
            Some(root) => {
                let runtime = spfs::runtime::Runtime::new(root)?;
                std::env::set_var("SPFS_RUNTIME", runtime.name());
                Some(spfs::runtime::OwnedRuntime::upgrade(runtime)?)
            }
            None => {
                std::env::remove_var("SPFS_RUNTIME");
                None
            }
        };

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
