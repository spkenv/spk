// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::{IntoDiagnostic, Result};
use spk_cli_common::{CommandArgs, Run};

/// Output the current configuration of spk
#[derive(Args)]
pub struct Config {}

#[async_trait::async_trait]
impl Run for Config {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let config = spk_config::get_config()?;
        let out = serde_json::to_string_pretty(&*config).into_diagnostic()?;
        println!("{out}");
        Ok(0)
    }
}

impl CommandArgs for Config {
    fn get_positional_args(&self) -> Vec<String> {
        // There are no important positional args for a config command
        vec![]
    }
}
