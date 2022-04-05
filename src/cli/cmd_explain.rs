// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

use super::flags;

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

    /// The requests to resolve
    #[clap(name = "REQUESTS", required = true)]
    pub requested: Vec<String>,
}

impl Explain {
    pub fn run(&self) -> Result<i32> {
        self.runtime.ensure_active_runtime()?;

        let mut solver = self.solver.get_solver(&self.options)?;
        let requests = self
            .requests
            .parse_requests(&self.requested, &self.options)?;
        for request in requests {
            solver.add_request(request)
        }

        let solution = spk::io::run_and_print_resolve(&solver, self.verbose + 1)?;

        eprintln!("{}", spk::io::format_solution(&solution, self.verbose + 1));
        Ok(0)
    }
}
