// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;

use clap::{Args, Subcommand};
use miette::Result;

/// View and manage spfs runtime information
#[derive(Debug, Args)]
#[clap(visible_alias = "rt")]
pub struct CmdRuntime {
    #[clap(subcommand)]
    pub(crate) command: Command,
}

impl CmdRuntime {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        self.command.run(config).await
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Info(super::cmd_runtime_info::CmdRuntimeInfo),
    List(super::cmd_runtime_list::CmdRuntimeList),
    Prune(super::cmd_runtime_prune::CmdRuntimePrune),
    Remove(super::cmd_runtime_remove::CmdRuntimeRemove),
}

impl Command {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        match self {
            Self::Info(cmd) => cmd.run(config).await,
            Self::List(cmd) => cmd.run(config).await,
            Self::Prune(cmd) => cmd.run(config).await,
            Self::Remove(cmd) => cmd.run(config).await,
        }
    }

    // Helper to get the repos.repo_path for the subcommand
    pub fn add_proxy_repo_arg(&self) -> &Option<PathBuf> {
        match self {
            Self::Info(cmd) => &cmd.repos.add_proxy_repo,
            Self::List(cmd) => &cmd.repos.add_proxy_repo,
            Self::Prune(cmd) => &cmd.repos.add_proxy_repo,
            Self::Remove(cmd) => &cmd.repos.add_proxy_repo,
        }
    }
}
