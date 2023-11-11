// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;
use std::time::Instant;

use clap::{ArgGroup, Args};
use miette::{Context, Result};
use spfs::storage::FromConfig;
use spfs::tracking::EnvSpec;
use spfs_cli_common as cli;

/// Run a program in a configured spfs environment
#[derive(Debug, Args)]
#[clap(group(
    ArgGroup::new("runtime_id")
    .required(true)
        .args(&["rerun", "reference"])))]
pub struct CmdRun {
    #[clap(flatten)]
    pub sync: cli::Sync,

    #[clap(flatten)]
    pub logging: cli::Logging,

    /// Mount the spfs filesystem in edit mode (default if REF is empty or not given)
    #[clap(short, long)]
    pub edit: bool,

    /// Mount the spfs filesystem in read-only mode (default if REF is non-empty)
    #[clap(long, overrides_with = "edit")]
    pub no_edit: bool,

    /// Requires --rerun. Force reset the process fields of the
    /// runtime before it is run again
    #[clap(long, requires = "rerun")]
    pub force: bool,

    /// Use to keep the runtime around rather than deleting it when
    /// the process exits. This is best used with '--name NAME' to
    /// make rerunning the runtime easier at a later time.
    #[clap(short, long, env = "SPFS_KEEP_RUNTIME")]
    pub keep_runtime: bool,

    /// Provide a name for this runtime to make it easier to identify
    #[clap(long)]
    pub runtime_name: Option<String>,

    /// Name of an existing durable runtime to reuse for this run
    #[clap(long, value_name = "RUNTIME_NAME")]
    pub rerun: Option<String>,

    /// The tag or id of the desired runtime
    ///
    /// Use '-' to request an empty environment
    pub reference: Option<spfs::tracking::EnvSpec>,

    /// The command to run in the environment and its arguments
    ///
    /// In order to ensure that flags are passed as-is, '--' must be
    /// place before specifying the command and any flags that should
    /// be given to that command:
    /// e.g. `spfs run <args> -- command --flag-for-command`
    #[arg(last = true, value_name = "COMMAND")]
    pub command: Vec<OsString>,
}

impl CmdRun {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let (repo, runtimes) = tokio::try_join!(
            config.get_local_repository_handle(),
            config.get_runtime_storage()
        )?;
        tracing::debug!("got local repository handle and runtime storage");

        if let Some(runtime_name) = &self.rerun {
            tracing::debug!("reading existing durable runtime: {runtime_name}");
            let mut runtime = runtimes
                .read_runtime(runtime_name)
                .await
                .map_err(Into::<miette::Error>::into)?;

            tracing::debug!(
                "existing durable runtime loaded with status: {}",
                runtime.status.running
            );

            if self.force {
                // Reset the runtime so it can be rerun. This is
                // usually done in the spfs monitor, unless something
                // went wrong when the runtime last exited.  The
                // --force flag allows runtimes left in a strange
                // state to be reset and rerun.
                runtime.reinit_for_reuse_and_save_to_storage().await?;
            }

            // TODO: there's nothing to change or clear any extra
            // mounts made the last time this durable runtime was run.
            // Currently, will use the extra mounts already in the runtime.

            let start_time = Instant::now();
            let origin = config.get_remote("origin").await?;
            let references_to_sync = EnvSpec::from_iter(runtime.status.stack.iter().copied());
            let _synced = self
                .sync
                .get_syncer(&origin, &repo)
                .sync_env(references_to_sync)
                .await?;
            tracing::debug!("synced and about to launch process with durable runtime");

            self.exec_runtime_command(&mut runtime, &start_time).await
        } else if let Some(reference) = &self.reference {
            let live_layers = reference.load_live_layers()?;
            if !live_layers.is_empty() {
                tracing::debug!("with live layers: {live_layers:?}");
            };

            // Make a new empty runtime
            let mut runtime = match &self.runtime_name {
                Some(name) => {
                    runtimes
                        .create_named_runtime(name, self.keep_runtime, live_layers)
                        .await?
                }
                None => {
                    runtimes
                        .create_runtime(self.keep_runtime, live_layers)
                        .await?
                }
            };

            if self.keep_runtime && self.runtime_name.is_none() {
                // User wants a durable runtime but has not named it,
                // which means it will get uuid name. Want to make
                // them aware of this and how to name it next time.
                tracing::warn!(
                    "created new durable runtime without naming it. You can use --runtime-name NAME to give the runtime a name. This one will be called: {}",
                    runtime.name()
                );
            } else {
                tracing::debug!(
                    "created new runtime: {} [keep={}]",
                    runtime.name(),
                    self.keep_runtime
                );
            }

            let start_time = Instant::now();
            runtime.config.mount_backend = config.filesystem.backend;
            runtime.config.secondary_repositories = config.get_secondary_runtime_repositories();
            if reference.is_empty() && !self.no_edit {
                self.edit = true;
            } else if runtime.config.mount_backend.requires_localization() {
                let origin = config.get_remote("origin").await?;
                // Convert the tag items in the reference field to their
                // underlying digests so the tags are not synced to the
                // local repo. Tags synced to a local repo will prevent
                // future 'spfs clean's from removing many unused spfs
                // objects.
                let repos: Vec<_> = vec![&*origin, &*repo];
                let references_to_sync = reference
                    .with_tag_items_resolved_to_digest_items(&repos)
                    .await?;
                let synced = self
                    .sync
                    .get_syncer(&origin, &repo)
                    .sync_env(references_to_sync)
                    .await?;
                for item in synced.env.iter() {
                    let digest = item.resolve_digest(&repo).await?;
                    runtime.push_digest(digest);
                }
            } else {
                //runtime.config.secondary_repositories = config.get_secondary_runtime_repositories();
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
                    .wrap_err("Failed to build proxy repository for environment resolution")?;
                for item in reference.iter() {
                    let digest = item.resolve_digest(&repo).await?;
                    runtime.push_digest(digest);
                }
            }
            tracing::debug!("synced all the referenced objects locally");

            self.exec_runtime_command(&mut runtime, &start_time).await
        } else {
            // Guaranteed by Clap config.
            unreachable!();
        }
    }

    async fn exec_runtime_command(
        &mut self,
        runtime: &mut spfs::runtime::Runtime,
        start_time: &Instant,
    ) -> Result<i32> {
        let command = match self.command.first() {
            Some(c) => c.clone(),
            None => Default::default(),
        };

        // The skip(1)'s filter "command" entry out of the processing
        runtime.status.command = vec![command.to_string_lossy().to_string()];
        runtime.status.command.extend(
            self.command
                .iter()
                .skip(1)
                .map(|s| s.to_string_lossy().to_string()),
        );
        runtime.status.editable = self.edit;
        runtime.save_state_to_storage().await?;

        tracing::debug!("resolving entry process");

        let cmd =
            spfs::build_command_for_runtime(runtime, command, self.command.drain(..).skip(1))?;

        let sync_time = start_time.elapsed();
        std::env::set_var(
            "SPFS_METRICS_SYNC_TIME_SECS",
            sync_time.as_secs_f64().to_string(),
        );

        cmd.exec()
            .map(|_| 0)
            .wrap_err("Failed to execute runtime command")
    }
}
