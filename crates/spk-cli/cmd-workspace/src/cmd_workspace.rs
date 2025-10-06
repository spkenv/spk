// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::{Args, Subcommand};
use miette::Result;
use spk_cli_common::{CommandArgs, Run};

/// Query and operate on an spk workspace directory.
#[derive(Args, Clone)]
#[clap(visible_aliases = &["ws", "w"])]
pub struct Workspace {
    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    Info(crate::info::Info),
    Build(crate::build::Build),
}

#[async_trait::async_trait]
impl Run for Workspace {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        match &mut self.cmd {
            Command::Info(cmd) => cmd.run().await,
            Command::Build(cmd) => cmd.run().await,
        }
    }
}

impl CommandArgs for Workspace {
    fn get_positional_args(&self) -> Vec<String> {
        match &self.cmd {
            Command::Info(cmd) => cmd.get_positional_args(),
            Command::Build(cmd) => cmd.get_positional_args(),
        }
    }
}
