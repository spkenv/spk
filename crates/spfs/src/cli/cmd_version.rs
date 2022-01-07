// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CmdVersion {}

impl CmdVersion {
    pub async fn run(&self) -> spfs::Result<i32> {
        println!("{}", spfs::VERSION);
        Ok(0)
    }
}
