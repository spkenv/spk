// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;

use spfs::prelude::*;

/// Check a repositories internal integrity
#[derive(Debug, Args)]
pub struct CmdCheck {
    /// Trigger the check operation on a remote repository instead of the local one
    #[clap(short, long)]
    remote: Option<String>,

    /// Attempt to fix problems by pulling from another repository. Defaults to "origin".
    #[clap(long)]
    pull: Option<Option<String>>,
}

impl CmdCheck {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let _pull_from = match self.pull.take() {
            Some(name @ Some(_)) if name == self.remote => {
                return Err("Cannot --pull from same repo as --remote".into());
            }
            Some(None)
                if self
                    .remote
                    .as_ref()
                    .map(|r| r == "origin")
                    .unwrap_or_default() =>
            {
                return Err("Cannot --pull from same repo as --remote".into());
            }
            Some(mut repo) => Some(
                spfs::config::open_repository_from_string(
                    config,
                    repo.take().or_else(|| Some("origin".to_owned())),
                )
                .await?,
            ),
            None => None,
        };

        tracing::info!("walking repository...");
        let errors = match repo {
            RepositoryHandle::FS(repo) => spfs::graph::check_database_integrity(repo).await,
            RepositoryHandle::Tar(repo) => spfs::graph::check_database_integrity(repo).await,
            RepositoryHandle::Rpc(repo) => spfs::graph::check_database_integrity(repo).await,
            RepositoryHandle::Proxy(repo) => spfs::graph::check_database_integrity(&*repo).await,
        };
        for error in errors.iter() {
            tracing::error!("{:?}", error);
        }
        if !errors.is_empty() {
            return Ok(1);
        }
        tracing::info!("repository OK");
        Ok(0)
    }
}
