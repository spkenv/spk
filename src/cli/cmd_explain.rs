// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

use super::{flags, Run};

/// Show the resolve process for a set of packages.
#[derive(Args)]
pub struct Explain {
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

    /// The requests to resolve
    #[clap(name = "REQUESTS", required = true)]
    pub requested: Vec<String>,
}

#[async_trait::async_trait]
impl Run for Explain {
    async fn run(&mut self) -> Result<i32> {
        self.runtime.ensure_active_runtime().await?;

        let (mut solver, requests) = tokio::try_join!(
            self.solver.get_solver(&self.options),
            self.requests.parse_requests(&self.requested, &self.options)
        )?;
        for request in requests {
            solver.add_request(request)
        }

        // Always show the solution packages for the solve
        let formatter = self
            .formatter_settings
            .get_formatter_builder(self.verbose + 1)
            .with_solution(true)
            .build();
        formatter.run_and_print_resolve(&solver).await?;

        Ok(0)
    }
}
