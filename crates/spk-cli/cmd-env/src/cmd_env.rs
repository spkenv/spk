// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;
use std::ffi::OsString;

use clap::Args;
use miette::{Context, Result};
use spfs::tracking::SpecFile;
use spfs_cli_common::Progress;
use spk_cli_common::{build_required_packages, flags, CommandArgs, Run};
use spk_exec::setup_runtime_with_reporter;
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

    /// Options for showing progress
    #[clap(long, value_enum)]
    pub progress: Option<Progress>,
}

#[async_trait::async_trait]
impl Run for Env {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let mut rt = self
            .runtime
            .ensure_active_runtime(&["env", "run", "shell"])
            .await?;

        rt.status.editable = self.runtime.editable();

        if let Some(live_layer_files) = &self.runtime.live_layer {
            // This is the equivalent of load_live_layers() without an EnvSpec
            let mut live_layers = Vec::new();
            for filepath in live_layer_files.iter() {
                if let SpecFile::LiveLayer(live_layer) = SpecFile::parse(filepath)? {
                    live_layers.push(live_layer);
                }
            }
            rt.config.live_layers = live_layers;
        }

        let mut solver = self.solver.get_solver(&self.options).await?;

        let (requests, extra_options) = self
            .requests
            .parse_requests(&self.requested, &self.options, solver.repositories())
            .await?;
        solver.update_options(extra_options);
        for request in requests {
            solver.add_request(request)
        }

        let formatter = self.formatter_settings.get_formatter(self.verbose)?;
        let (solution, _) = formatter.run_and_print_resolve(&solver).await?;

        let solution = build_required_packages(&solution).await?;

        rt.status.editable =
            self.runtime.editable() || self.requests.any_build_stage_requests(&self.requested)?;
        setup_runtime_with_reporter(&mut rt, &solution, {
            match self.progress.unwrap_or_default() {
                Progress::Bars => spfs::sync::reporter::SyncReporters::console,
                Progress::None => spfs::sync::reporter::SyncReporters::silent,
            }
        })
        .await?;

        let env = solution.to_environment(Some(std::env::vars()));

        let mut command = if self.command.is_empty() {
            spfs::build_interactive_shell_command(&rt, None)?
        } else {
            let cmd = self.command.first().unwrap();
            let args = &self.command[1..];
            spfs::build_shell_initialized_command(&rt, None, cmd, args)?
        };

        // Previously we modified the existing environment but that is not
        // safe. The changes that `solution.to_environment` makes to the
        // environment, e.g., `$SPK_*` vars, should not impact the behavior of
        // `spfs::build_interactive_shell_command` or
        // `spfs::build_shell_initialized_command`.
        // Preserve any vars already established in `command.vars`.
        let existing_new_vars = command.vars.iter().map(|(k, _)| k).collect::<HashSet<_>>();
        command.vars.extend(
            env.into_iter()
                .filter_map(|(k, v)| {
                    let k: OsString = k.into();
                    (!existing_new_vars.contains(&k)).then(|| (k, v.into()))
                })
                .collect::<Vec<_>>(),
        );

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
            .wrap_err("Failed to execute runtime command")
    }
}

impl CommandArgs for Env {
    fn get_positional_args(&self) -> Vec<String> {
        self.requested.clone()
    }
}
