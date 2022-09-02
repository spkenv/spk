// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::ffi::OsString;

use anyhow::{Context, Result};
use clap::Args;
use spk_cli_common::{build_required_packages, flags, CommandArgs, Run};
use spk_exec::setup_runtime;

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

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

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

#[async_trait::async_trait]
impl Run for Env {
    async fn run(&mut self) -> Result<i32> {
        let mut rt = self
            .runtime
            .ensure_active_runtime(&["env", "run", "shell"])
            .await?;

        let (mut solver, requests) = tokio::try_join!(
            self.solver.get_solver(&self.options),
            self.requests.parse_requests(&self.requested, &self.options)
        )?;
        for request in requests {
            solver.add_request(request)
        }

        let formatter = self.formatter_settings.get_formatter(self.verbose);
        let solution = formatter.run_and_print_resolve(&solver).await?;

        let solution = build_required_packages(&solution).await?;
        setup_runtime(&mut rt, &solution).await?;

        let env = solution.to_environment(Some(std::env::vars()));
        let _: Vec<_> = std::env::vars()
            .map(|(k, _)| k)
            .map(std::env::remove_var)
            .collect();
        for (name, value) in env.into_iter() {
            std::env::set_var(name, value);
        }

        let command = if self.command.is_empty() {
            spfs::build_interactive_shell_command(&rt)?
        } else {
            let cmd = self.command.get(0).unwrap();
            let args = &self.command[1..];
            spfs::build_shell_initialized_command(&rt, cmd, args)?
        };
        self.run_command(command.executable, command.args)
    }
}

impl CommandArgs for Env {
    fn get_positional_args(&self) -> Vec<String> {
        self.requested.clone()
    }
}

impl Env {
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
