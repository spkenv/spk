// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use miette::{IntoDiagnostic, Result};

/// Output the current configuration of spfs
#[derive(Debug, Args)]
pub struct CmdConfig {}

impl CmdConfig {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let out = serde_json::to_string_pretty(&config).into_diagnostic()?;
        println!("{out}");
        Ok(0)
    }
}
