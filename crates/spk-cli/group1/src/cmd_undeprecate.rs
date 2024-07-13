// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spk_cli_common::{flags, CommandArgs, Run};

use super::cmd_deprecate::{change_deprecation_state, ChangeAction};

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
#[async_trait::async_trait]
impl Run for Undeprecate {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        change_deprecation_state(
            ChangeAction::Undeprecate,
            &self.repos.get_repos_for_destructive_operation().await?,
            &self.packages,
            self.yes,
        )
        .await
    }
}

impl CommandArgs for Undeprecate {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for an undeprecate are the packages
        self.packages.clone()
    }
}
