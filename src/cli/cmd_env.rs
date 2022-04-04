// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::ffi::OsString;

use anyhow::{Context, Result};
use clap::Args;

use super::flags;

/// Resolve and run an environment on-the-fly
///
/// Use '--' to separate the command from requests. If no command is given,
/// spawn a new shell
#[derive(Args)]
#[clap(visible_aliases = &["run", "shell"])]
pub struct Env {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// The requests to resolve and run
    #[clap(name = "REQUESTS")]
    pub requested: Vec<String>,

    /// An optional command to run in the resolved environment.
    ///
    /// Use '--' to separate the command from requests. If no command is given,
    /// spawn a new shell
    #[clap(raw = true)]
    pub command: Vec<String>,
}

impl Env {
    pub fn run(&self) -> Result<i32> {
        self.runtime.ensure_active_runtime()?;

        let mut solver = self.solver.get_solver(&self.options)?;
        let requests = self
            .requests
            .parse_requests(&self.requested, &self.options)?;
        for request in requests {
            solver.add_request(request)
        }

        let solution = spk::io::run_and_print_resolve(&solver, self.verbose)?;

        if self.verbose > 0 {
            eprintln!("{}", spk::io::format_solution(&solution, self.verbose));
        }

        let solution = spk::build_required_packages(&solution)?;
        spk::setup_current_runtime(&solution)?;
        let env = solution.to_environment(Some(std::env::vars()));
        let _: Vec<_> = std::env::vars()
            .map(|(k, _)| k)
            .map(std::env::remove_var)
            .collect();
        for (name, value) in env.into_iter() {
            std::env::set_var(name, value);
        }

        let mut command = if self.command.is_empty() {
            let rt = spfs::active_runtime()?;
            spfs::build_interactive_shell_cmd(&rt)?
        } else {
            let cmd = std::ffi::OsString::from(self.command.get(0).unwrap());
            let mut args = self.command[1..]
                .iter()
                .map(std::ffi::OsString::from)
                .collect();
            spfs::build_shell_initialized_command(cmd, &mut args)?
        };
        let exe = command.drain(..1).next().unwrap();
        self.run_command(exe, command)
    }

    #[cfg(target_os = "linux")]
    pub fn run_command(&self, exe: OsString, args: Vec<OsString>) -> Result<i32> {
        use std::os::unix::ffi::OsStrExt;

        let exe = std::ffi::CString::new(exe.as_bytes())
            .context("Provided command was not a valid string")?;
        let mut args = args
            .iter()
            .map(|arg| std::ffi::CString::new(arg.as_bytes()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("One or more arguments was not a valid c-string")?;
        args.insert(0, exe.clone());

        tracing::trace!("{:?}", args);
        nix::unistd::execvp(&exe, args.as_slice()).context("Command failed to launch")?;
        unreachable!()
    }
}
