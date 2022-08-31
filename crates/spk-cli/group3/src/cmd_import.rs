// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{Context, Result};
use clap::Args;
use futures::TryStreamExt;

use spk_cli_common::{CommandArgs, Run};

#[cfg(test)]
#[path = "./cmd_import_test.rs"]
mod cmd_import_test;

/// Import a previously exported package/archive
#[derive(Args)]
pub struct Import {
    #[clap(flatten)]
    sync: spfs_cli_common::Sync,

    /// The archive to import from
    #[clap(name = "FILE")]
    pub files: Vec<std::path::PathBuf>,
}

#[async_trait::async_trait]
impl Run for Import {
    async fn run(&mut self) -> Result<i32> {
        let mut summary = spfs::sync::SyncSummary::default();
        let local_repo = spk_storage::local_repository().await?;
        // src and dst are the same here which is useless, but we will
        // be using this syncer to create more useful ones for each archive
        let syncer = self.sync.get_syncer(&local_repo, &local_repo);
        for filename in self.files.iter() {
            let tar_repo = spfs::storage::tar::TarRepository::open(&filename).await?;
            let tar_repo: spfs::storage::RepositoryHandle = tar_repo.into();
            let env_spec = tar_repo
                .iter_tags()
                .map_ok(|(spec, _)| spec)
                .try_collect()
                .await
                .context("Failed to collect tags from archive")?;
            tracing::info!(archive = ?filename, "importing");
            summary += syncer
                .clone_with_source(&tar_repo)
                .sync_env(env_spec)
                .await
                .context("Failed to sync archived data")?
                .summary();
        }
        tracing::info!("{:#?}", summary);
        Ok(0)
    }
}

impl CommandArgs for Import {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for an import are the archive files
        self.files
            .iter()
            .map(|p| format!("{}", p.display()))
            .collect::<Vec<String>>()
    }
}
