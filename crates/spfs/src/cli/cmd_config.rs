// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CmdConfig {}

impl CmdConfig {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let out = serde_json::to_string_pretty(&config)?;
        println!("{out}");
        Ok(0)
    }
}
