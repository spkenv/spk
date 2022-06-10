// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{Context, Result};
use clap::Args;

use super::{flags, Run};

/// Bake an executable environment from a set of requests or the current environment.
#[derive(Args)]
pub struct Bake {
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// The requests to resolve and bake
    #[clap(name = "REQUESTS")]
    pub requested: Vec<String>,
}

impl Run for Bake {
    fn run(&mut self) -> Result<i32> {
        if self.requested.is_empty() {
            let rt = spk::HANDLE.block_on(spfs::active_runtime())?;
            for layer in rt.status.stack.iter() {
                println!("{layer}");
            }
            Ok(0)
        } else {
            self.runtime.ensure_active_runtime()?;
            self.solve_and_build_new_runtime()
        }
    }
}

impl Bake {
    fn solve_and_build_new_runtime(&self) -> Result<i32> {
        let exe = std::env::current_exe()?
            .to_str()
            .map(String::from)
            .context("Failed converting current executable path to a string")?;
        let mut env = super::cmd_env::Env {
            solver: self.solver.clone(),
            runtime: self.runtime.clone(),
            requests: self.requests.clone(),
            options: self.options.clone(),
            verbose: self.verbose,
            formatter_settings: self.formatter_settings.clone(),
            requested: self.requested.clone(),
            command: vec![exe, "bake".into()],
        };
        env.run()
    }
}
