// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

use super::flags;

/// Convert a package from an external packaging system for use in spk
#[derive(Args)]
pub struct Convert {
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

    /// The converter to run
    converter: String,

    /// Arguments to the conversion command, separated with '--'.
    ///
    /// If you are not sure what arguments are available, you can
    /// always run it with the --help argument.
    #[clap(raw = true)]
    args: Vec<String>,
}

impl Convert {
    pub fn run(&self) -> Result<i32> {
        let converter_package = format!("spk-convert-{}", self.converter);

        let mut command = vec![converter_package.clone()];
        command.extend(self.args.clone());

        let env = super::cmd_env::Env {
            solver: self.solver.clone(),
            options: self.options.clone(),
            runtime: self.runtime.clone(),
            requests: self.requests.clone(),
            verbose: self.verbose,
            requested: vec![converter_package],
            command,
        };
        env.run()
    }
}
