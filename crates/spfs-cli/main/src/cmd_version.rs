// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;

/// Print the version of spfs
#[derive(Debug, Args)]
pub struct CmdVersion {}

impl CmdVersion {
    pub async fn run(&self) -> Result<i32> {
        println!("{}", spfs::VERSION);
        Ok(0)
    }
}
