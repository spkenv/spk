// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::{Args, Subcommand};
use miette::{Context, Result};
use spk_cli_common::{CommandArgs, Run};
use spk_storage as storage;
use storage::Repository;

/// Perform repository-level actions and maintenance
#[derive(Args)]
pub struct Repo {
    #[clap(subcommand)]
    command: RepoCommand,
}

#[async_trait::async_trait]
impl Run for Repo {
    async fn run(&mut self) -> Result<i32> {
        self.command.run().await
    }
}

impl CommandArgs for Repo {
    fn get_positional_args(&self) -> Vec<String> {
        // There are no important positional args for the repo command
        vec![]
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
        /// The repository to upgrade (name or path or url)
        #[clap(name = "REPO")]
        repo: String,
    },
}

impl RepoCommand {
    pub async fn run(&mut self) -> Result<i32> {
        let repo = match &self {
            Self::Upgrade { repo } => repo,
        };
        let repo = match repo.as_str() {
            "local" => storage::local_repository().await?,
            _ => storage::remote_repository(repo).await?,
        };
        let status = repo.upgrade().await.wrap_err("Upgrade failed")?;
        tracing::info!("{}", status);
        Ok(1)
    }
}
