// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spfs_cli_common::Progress;
use spk_cli_common::{CommandArgs, Run, flags};

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

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Options for showing progress
    #[clap(long, value_enum)]
    pub progress: Option<Progress>,

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
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let converter_package = format!("spk-convert-{}", self.converter);

        let mut command = vec![converter_package.clone()];
        command.extend(self.args.clone());
        if self.verbose > 0 {
            // Pass the verbosity level into the conversion command
            command.push(format!("-{}", "v".repeat(self.verbose.into())));
        }
        tracing::debug!("Underlying command: {}", command.join(" "));

        let mut env = spk_cmd_env::cmd_env::Env {
            solver: self.solver.clone(),
            options: self.options.clone(),
            runtime: self.runtime.clone(),
            requests: self.requests.clone(),
            verbose: self.verbose,
            progress: self.progress,
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
