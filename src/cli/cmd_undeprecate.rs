// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

use crate::cmd_deprecate::{change_deprecation_state, ChangeAction};

use super::{flags, Run};

#[cfg(test)]
#[path = "./cmd_undeprecate_test.rs"]
mod cmd_undeprecate_test;

/// Undeprecate (restore) packages in a repository.
///
/// Undeprecated package builds can be resolved normally. They will
/// show up in environments. By undeprecating a package version, as
/// opposed to an individual build, the package can be rebuilt from
/// source again and this undeprecates all builds by association.
#[derive(Args, Clone)]
pub struct Undeprecate {
    #[clap(flatten)]
    repos: flags::Repositories,

    /// If set, answer 'Yes' to all confirmation prompts
    #[clap(long, short)]
    pub yes: bool,

    /// The package version or build to undeprecate
    ///
    /// By undeprecating a package version, as opposed to an
    /// individual build, the package can be rebuilt from source. This
    /// also undeprecates all builds by association.
    #[clap(name = "PKG", required = true)]
    packages: Vec<String>,
}

/// Undeprecates package builds in a repository.
impl Run for Undeprecate {
    fn run(&mut self) -> Result<i32> {
        change_deprecation_state(
            ChangeAction::Undeprecate,
            &self.repos.get_repos(None)?,
            &self.packages,
            self.yes,
        )
    }
}
