// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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
