// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

use super::Run;

/// Print the spk version information
#[derive(Args)]
pub struct Version {}

impl Run for Version {
    fn run(&mut self) -> Result<i32> {
        println!(" spk {}", spk::VERSION);
        println!("spfs {}", spfs::VERSION);
        Ok(0)
    }
}
