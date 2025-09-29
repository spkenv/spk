// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::{Args, Subcommand};
use miette::Result;
use spk_cli_common::{CommandArgs, Run};

use crate::info::Info;

/// Query and operate on an spk workspace directory.
#[derive(Args, Clone)]
#[clap(visible_aliases = &["ws", "w"])]
pub struct Workspace {
    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand, Clone)]
pub enum Command {
    Info(Info),
}

#[async_trait::async_trait]
impl Run for Workspace {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        match &mut self.cmd {
            Command::Info(info) => info.run().await,
        }
    }
}

impl CommandArgs for Workspace {
    fn get_positional_args(&self) -> Vec<String> {
        match &self.cmd {
            Command::Info(info) => info.get_positional_args(),
        }
    }
}
