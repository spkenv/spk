// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

/// Print the spk version information
#[derive(Args)]
pub struct Version {}

impl Version {
    pub fn run(&self) -> Result<i32> {
        println!(" spk {}", spk::VERSION);
        println!("spfs {}", spfs::VERSION);
        Ok(0)
    }
}
