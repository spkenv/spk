// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;
use spk_cli_common::{CommandArgs, Run, VERSION};

/// Print the spk version information
#[derive(Args)]
pub struct Version {}

#[async_trait::async_trait]
impl Run for Version {
    async fn run(&mut self) -> Result<i32> {
        println!(" spk {}", VERSION);
        println!("spfs {}", spfs::VERSION);
        Ok(0)
    }
}

impl CommandArgs for Version {
    fn get_positional_args(&self) -> Vec<String> {
        // There are no important positional args for a version command
        vec![]
    }
}
