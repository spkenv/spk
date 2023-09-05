// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{Context, Result};
use clap::Args;
use spk_cli_common::{build_required_packages, flags, CommandArgs, Run};
use spk_exec::setup_runtime;
#[cfg(feature = "statsd")]
use spk_solve::{get_metrics_client, SPK_RUN_TIME_METRIC};

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

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

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

        let mut solver = self.solver.get_solver(&self.options).await?;

        let requests = self
            .requests
            .parse_requests(&self.requested, &self.options, solver.repositories())
            .await?;
        for request in requests {
            solver.add_request(request)
        }

        let formatter = self.formatter_settings.get_formatter(self.verbose)?;
        let (solution, _) = formatter.run_and_print_resolve(&solver).await?;

        let solution = build_required_packages(&solution).await?;

        rt.status.editable =
            self.runtime.editable() || self.requests.any_build_stage_requests(&self.requested)?;
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
            spfs::build_interactive_shell_command(&rt, None)?
        } else {
            let cmd = self.command.get(0).unwrap();
            let args = &self.command[1..];
            spfs::build_shell_initialized_command(&rt, None, cmd, args)?
        };

        // Record the run duration up to this point because this spk
        // command is about to replace itself with the underlying env
        // command and we want to capture data on this spk processes
        // part of the run, but not the time spent in the underlying,
        // possibly long running, command/env (e.g. shell or application).
        #[cfg(feature = "statsd")]
        {
            if let Some(statsd_client) = get_metrics_client() {
                statsd_client.record_duration_from_start(&SPK_RUN_TIME_METRIC);
            }
        }

        command
            .exec()
            .map(|_| 0)
            .context("Failed to execute runtime command")
    }
}

impl CommandArgs for Env {
    fn get_positional_args(&self) -> Vec<String> {
        self.requested.clone()
    }
}
