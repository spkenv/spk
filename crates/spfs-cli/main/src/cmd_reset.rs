// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;
use spfs_cli_common as cli;

/// Reset changes, or rebuild the entire spfs directory
#[derive(Args, Debug)]
pub struct CmdReset {
    #[clap(flatten)]
    sync: cli::Sync,

    /// Mount the resulting runtime in edit mode
    ///
    /// Default to true if REF is empty or not given
    #[clap(short, long)]
    edit: bool,

    /// The tag or id to rebuild the runtime with.
    ///
    /// Uses current runtime stack if not given. Use '-' or
    /// an empty string to request an empty environment. Only valid
    /// if no paths are given
    #[clap(long = "ref", short)]
    reference: Option<spfs::tracking::EnvSpec>,

    /// Glob patterns in the spfs dir of files to reset, defaults to everything
    paths: Vec<String>,
}

impl CmdReset {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        #[rustfmt::skip]
        let (mut runtime, repo) = tokio::try_join!(
            spfs::active_runtime(),
            config.get_local_repository_handle()
        )?;
        if let Some(env_spec) = &self.reference {
            runtime.reset::<&str>(&[])?;
            runtime.status.stack.truncate(0);
            if env_spec.is_empty() {
                self.edit = true;
            } else {
                let origin = config.get_remote("origin").await?;
                let synced = self
                    .sync
                    .get_syncer(&origin, &repo)
                    .sync_env(env_spec.clone())
                    .await?;
                for item in synced.env.iter() {
                    let digest = item.resolve_digest(&*repo).await?;
                    runtime.push_digest(digest);
                }
            }
        } else {
            let paths = strip_spfs_prefix(&self.paths);
            runtime.reset(paths.as_slice())?;
        }

        if self.edit {
            runtime.status.editable = true;
        }

        runtime.save_state_to_storage().await?;
        spfs::remount_runtime(&runtime).await?;
        Ok(0)
    }
}

fn strip_spfs_prefix(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            path.strip_prefix("/spfs")
                .unwrap_or_else(|| path.as_ref())
                .to_owned()
        })
        .collect()
}
