// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;

/// Output the current configuration of spfs
#[derive(Debug, Args)]
pub struct CmdConfig {}

impl CmdConfig {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let out = serde_json::to_string_pretty(&config)?;
        println!("{out}");
        Ok(0)
    }
}
