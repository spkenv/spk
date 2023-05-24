// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Args;
use spfs::storage::FromConfig;
use spfs_cli_common as cli;

/// Run a program in a configured spfs environment
#[derive(Debug, Args)]
pub struct CmdRun {
    #[clap(flatten)]
    pub sync: cli::Sync,

    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Mount the spfs filesystem in edit mode (true if REF is empty or not given)
    #[clap(short, long)]
    pub edit: bool,

    /// Provide a name for this runtime to make it easier to identify
    #[clap(short, long)]
    pub name: Option<String>,

    /// The tag or id of the desired runtime
    ///
    /// Use '-' or an empty string to request an empty environment
    pub reference: spfs::tracking::EnvSpec,

    /// The command to run in the environment
    pub command: OsString,

    /// Additional arguments to provide to the command
    ///
    /// In order to ensure that flags are passed as-is, place '--' before
    /// specifying any flags that should be given to the subcommand:
    ///   eg `spfs enter <args> -- command --flag-for-command`
    pub args: Vec<OsString>,
}

impl CmdRun {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let (repo, runtimes) = tokio::try_join!(
            config.get_local_repository_handle(),
            config.get_runtime_storage()
        )?;
        let mut runtime = match &self.name {
            Some(name) => runtimes.create_named_runtime(name).await?,
            None => runtimes.create_runtime().await?,
        };
        tracing::debug!("created runtime: {}", runtime.name());

        let start_time = Instant::now();
        runtime.config.mount_backend = config.filesystem.backend;
        if self.reference.is_empty() {
            self.edit = true;
        } else if runtime.config.mount_backend.requires_localization() {
            let origin = config.get_remote("origin").await?;
            // Convert the tag items in the reference field to their
            // underlying digests so the tags are not synced to the
            // local repo. Tags synced to a local repo will prevent
            // future 'spfs clean's from removing many unused spfs
            // objects.
            let repos: Vec<_> = vec![&*origin, &*repo];
            let references_to_sync = self
                .reference
                .with_tag_items_resolved_to_digest_items(&repos)
                .await?;
            let synced = self
                .sync
                .get_syncer(&origin, &repo)
                .sync_env(references_to_sync)
                .await?;
            for item in synced.env.iter() {
                let digest = item.resolve_digest(&*repo).await?;
                runtime.push_digest(digest);
            }
        } else {
            runtime.config.secondary_repositories = config.get_secondary_runtime_repositories();
            let proxy_config = spfs::storage::proxy::Config {
                primary: repo.address().to_string(),
                secondary: runtime
                    .config
                    .secondary_repositories
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            };
            let repo = spfs::storage::ProxyRepository::from_config(proxy_config)
                .await
                .context("Failed to build proxy repository for environment resolution")?;
            for item in self.reference.iter() {
                let digest = item.resolve_digest(&repo).await?;
                runtime.push_digest(digest);
            }
        }
        tracing::debug!("synced all the referenced objects locally");

        runtime.status.command = vec![self.command.to_string_lossy().to_string()];
        runtime
            .status
            .command
            .extend(self.args.iter().map(|s| s.to_string_lossy().to_string()));
        runtime.status.editable = self.edit;
        runtime.save_state_to_storage().await?;

        tracing::debug!("resolving entry process");
        let cmd = spfs::build_command_for_runtime(&runtime, &self.command, self.args.drain(..))?;

        let sync_time = start_time.elapsed();
        std::env::set_var(
            "SPFS_METRICS_SYNC_TIME_SECS",
            sync_time.as_secs_f64().to_string(),
        );

        cmd.exec()
            .map(|_| 0)
            .context("Failed to execute runtime command")
    }
}
