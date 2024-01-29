// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::process::ExitStatus;

use clap::Args;
use miette::Result;
use spk_cli_common::{CommandArgs, Run, VERSION};

/// Print the spk version information
#[derive(Args)]
pub struct Version {}

#[async_trait::async_trait]
impl Run for Version {
    type Output = ExitStatus;

    async fn run(&mut self) -> Result<Self::Output> {
        println!(" spk {VERSION}");
        println!("spfs {}", spfs::VERSION);
        Ok(ExitStatus::default())
    }
}

impl CommandArgs for Version {
    fn get_positional_args(&self) -> Vec<String> {
        // There are no important positional args for a version command
        vec![]
    }
}
