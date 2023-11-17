// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use miette::Result;
use spk_cli_common::{flags, CommandArgs, Run};

/// Show the resolve process for a set of packages.
#[derive(Args)]
pub struct Explain {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// The requests to resolve
    #[clap(name = "REQUESTS", required = true)]
    pub requested: Vec<String>,

    // The following arguments were previously provided by the `runtime` field.
    // These are now ignored however they are still accepted for backwards
    // compatibility and can be removed after a deprecation period.
    #[clap(long, hide = true)]
    pub no_runtime: bool,
    #[clap(long, hide = true)]
    pub edit: bool,
    #[clap(long, hide = true)]
    pub no_edit: bool,
    #[clap(long, hide = true)]
    pub runtime_name: Option<String>,
    #[clap(long, hide = true)]
    pub keep_runtime: bool,
    #[clap(long, hide = true)]
    pub live_layer: Option<Vec<spfs::runtime::LiveLayerFile>>,
}

#[async_trait::async_trait]
impl Run for Explain {
    async fn run(&mut self) -> Result<i32> {
        // Warn about deprecated arguments.
        if self.no_runtime {
            tracing::warn!("When using explain, --no-runtime is deprecated and has no effect");
        }
        if self.edit {
            tracing::warn!("When using explain, --edit is deprecated and has no effect");
        }
        if self.no_edit {
            tracing::warn!("When using explain, --no-edit is deprecated and has no effect");
        }
        if self.runtime_name.is_some() {
            tracing::warn!("When using explain, --runtime-name is deprecated and has no effect");
        }
        if self.keep_runtime {
            tracing::warn!("When using explain, --keep-runtime is deprecated and has no effect");
        }
        if self.live_layer.is_some() {
            tracing::warn!("When using explain, --live-layer is deprecated and has no effect");
        }

        let mut solver = self.solver.get_solver(&self.options).await?;

        let requests = self
            .requests
            .parse_requests(&self.requested, &self.options, solver.repositories())
            .await?;
        for request in requests {
            solver.add_request(request)
        }

        // Always show the solution packages for the solve
        let formatter = self
            .formatter_settings
            .get_formatter_builder(self.verbose + 1)?
            .with_solution(true)
            .build();
        formatter.run_and_print_resolve(&solver).await?;

        Ok(0)
    }
}

impl CommandArgs for Explain {
    fn get_positional_args(&self) -> Vec<String> {
        self.requested.clone()
    }
}
