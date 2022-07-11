// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

use super::{flags, CommandArgs, Run};

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

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// The converter to run
    converter: String,

    /// Arguments to the conversion command, separated with '--'.
    ///
    /// If you are not sure what arguments are available, you can
    /// always run it with the --help argument.
    #[clap(raw = true)]
    args: Vec<String>,
}

#[async_trait::async_trait]
impl Run for Convert {
    async fn run(&mut self) -> Result<i32> {
        let converter_package = format!("spk-convert-{}", self.converter);

        let mut command = vec![converter_package.clone()];
        command.extend(self.args.clone());

        let mut env = super::cmd_env::Env {
            solver: self.solver.clone(),
            options: self.options.clone(),
            runtime: self.runtime.clone(),
            requests: self.requests.clone(),
            verbose: self.verbose,
            formatter_settings: self.formatter_settings.clone(),
            requested: vec![converter_package],
            command,
        };
        env.run().await
    }
}

impl CommandArgs for Convert {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for a convert are the specified converter and args
        let mut tmp: Vec<String> = Vec::with_capacity(self.args.len() + 1);
        tmp.push(self.converter.clone());
        tmp.extend(self.args.clone());
        tmp
    }
}
