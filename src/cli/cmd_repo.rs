// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::ffi::OsString;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};

use super::flags;

/// Perform repository-level actions and maintenance
#[derive(Args)]
pub struct Repo {
    #[clap(subcommand)]
    command: RepoCommand,
}

impl Repo {
    pub fn run(&self) -> Result<i32> {
        self.command.run()
    }
}

#[derive(Subcommand)]
pub enum RepoCommand {
    /// Perform any pending upgrades to a package repository.
    ///
    /// This will bring the repository up-to-date for the current
    /// spk library version, but may also make it incompatible with
    /// older ones. Upgrades can also take time depending on their
    /// nature and the size of the repository so. Please, take time to
    /// read any release and upgrade notes before invoking this.
    Upgrade {
        // upgrade_cmd.add_argument(
    //     "repo", metavar="REPO", nargs=1, help="The repository to upgrade"
    // )
    },
}

impl RepoCommand {
    pub fn run(&self) -> Result<i32> {
        // repo = spk.storage.remote_repository(args.repo[0])
        // print(repo.upgrade())
        todo!()
    }
}
