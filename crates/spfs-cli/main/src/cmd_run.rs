// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::time::Instant;

use clap::{ArgGroup, Args};
use miette::{Context, IntoDiagnostic, Result, miette};
use spfs::graph::object::EncodingFormat;
use spfs::prelude::*;
use spfs::runtime::KeyValuePairBuf;
use spfs::storage::FromConfig;
use spfs::tracking::EnvSpec;
use spfs_cli_common as cli;

#[cfg(test)]
#[path = "./cmd_run_test.rs"]
mod cmd_run_test;
#[cfg(test)]
#[path = "./fixtures.rs"]
mod fixtures;

#[derive(Args, Clone, Debug)]
pub struct Annotation {
    /// Adds annotation key-value string data to the new runtime.
    ///
    /// This allows external processes to store arbitrary data in the
    /// runtimes they create. This is most useful with durable runtimes.
    /// The data can be retrieved by running `spfs runtime info` or
    /// `spfs info` and using the `--get <KEY>` or `--get-all` options
    ///
    /// Annotation data is specified as key-value string pairs
    /// separated by either an equals sign or colon (--annotation
    /// name=value --annotation other:value). Multiple pairs of
    /// annotation data can also be specified at once in yaml or json
    /// format (--annotation '{name: value, other: value}').
    ///
    /// Annotation data can also be given in a json or yaml file, by
    /// using the `--annotation-file <FILE>` argument. If given,
    /// `--annotation` arguments will supersede anything given in
    /// annotation files.
    ///
    /// If the same key is used more than once, the last key-value pair
    /// will override the earlier values for the same key.
    #[clap(long, value_name = "KEY:VALUE")]
    pub annotation: Vec<String>,

    /// Specify annotation key-value data from a json or yaml file
    /// (see --annotation)
    #[clap(long)]
    pub annotation_file: Vec<std::path::PathBuf>,
}

impl Annotation {
    /// Returns a list of annotation key-value pairs gathered from all
    /// the annotation related command line arguments. The same keys,
    /// and values, can appear multiple times in the list if specified
    /// multiple times in various command line arguments.
    pub fn get_data(&self) -> Result<Vec<KeyValuePairBuf>> {
        let mut data: Vec<KeyValuePairBuf> = Vec::new();

        for filename in self.annotation_file.iter() {
            let reader: Box<dyn io::Read> =
                if Ok("-".to_string()) == filename.clone().into_os_string().into_string() {
                    // Treat '-' as "read from stdin"
                    Box::new(io::stdin())
                } else {
                    Box::new(
                        std::fs::File::open(filename)
                            .into_diagnostic()
                            .wrap_err(format!("Failed to open annotation file: {filename:?}"))?,
                    )
                };
            let annotation: BTreeMap<String, String> = serde_yaml::from_reader(reader)
                .into_diagnostic()
                .wrap_err(format!(
                    "Failed to parse as annotation data key-value pairs: {filename:?}"
                ))?;
            data.extend(annotation);
        }

        for pair in self.annotation.iter() {
            let pair = pair.trim();
            if pair.starts_with('{') {
                let given: BTreeMap<String, String> = serde_yaml::from_str(pair)
                    .into_diagnostic()
                    .wrap_err("--annotation value looked like yaml, but could not be parsed")?;
                data.extend(given);
                continue;
            }

            let (name, value) = pair
                .split_once('=')
                .or_else(|| pair.split_once(':'))
                .ok_or_else(|| {
                    miette!("Invalid option: -annotation {pair} (should be in the form name=value)")
                })?;

            data.push((name.to_string(), value.to_string()));
        }

        Ok(data)
    }
}

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

    #[clap(flatten)]
    pub(crate) repos: cli::Repositories,

    /// Mount the spfs filesystem in edit mode.
    ///
    /// Editable runtimes are created by default if REF is empty or not given.
    /// When combined with --rerun, the original runtime editability is overridden
    #[clap(short, long)]
    pub edit: bool,

    /// Mount the spfs filesystem in read-only mode.
    ///
    /// Read-only runtimes are created by default if REF is provided and not empty.
    /// When combined with --rerun, the original runtime editability is overridden
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

    #[clap(flatten)]
    pub annotation: Annotation,

    /// The tag or id of the desired runtime
    ///
    /// Use '-' to or an empty string to request an empty environment
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
            if let Some(origin) = config.try_get_remote("origin").await? {
                let references_to_sync = EnvSpec::from_iter(runtime.status.stack.iter_bottom_up());
                let _synced = self
                    .sync
                    .get_syncer(&origin, &repo)
                    .sync_ref_spec(references_to_sync.try_into()?)
                    .await?;
            }
            tracing::debug!("synced and about to launch process with durable runtime");

            self.exec_runtime_command(&mut runtime, &start_time).await
        } else if let Some(reference) = &self.reference {
            let live_layers = reference.load_live_layers();
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

            let data = self.annotation.get_data()?;
            if !data.is_empty() {
                if config.storage.encoding_format == EncodingFormat::Legacy {
                    return Err(spfs::Error::String(
                        "Cannot use '--annotation' when spfs is configured to use the 'Legacy' encoding format".to_string(),
                    )
                    .into());
                }

                // These are added in reverse order so that the ones
                // specified later on the command line will take precedence.
                for (key, value) in data.into_iter().rev() {
                    tracing::trace!("annotation being added: {key}: {value}");
                    runtime
                        .add_annotation(&key, &value, config.filesystem.annotation_size_limit)
                        .await?;
                }
                tracing::trace!(" with annotation: {:?}", runtime);
            }

            let start_time = Instant::now();
            runtime.config.mount_backend = config.filesystem.backend;
            runtime.config.secondary_repositories = config.get_secondary_runtime_repositories();
            if reference.is_empty() && !self.no_edit {
                // when empty runtimes are created, we default to editable
                // since there are not files to protect and likely it's being
                // used to construct a new layer since it's useless otherwise
                //
                // note that this assumes that --rerun is handled separately above
                // because we don't want to be overriding the value of the existing
                // runtime unless the flag is explicitly given.
                self.edit = true;
            } else if runtime.config.mount_backend.requires_localization() {
                if let Some(origin) = config.try_get_remote("origin").await? {
                    // Convert the tag items in the reference field to their
                    // underlying digests so the tags are not synced to the
                    // local repo. Tags synced to a local repo will prevent
                    // future 'spfs clean's from removing many unused spfs
                    // objects.
                    let repos: Vec<_> = vec![&origin, &repo];
                    let references_to_sync = reference
                        .with_tag_items_resolved_to_digest_items(&repos)
                        .await?;
                    let synced = self
                        .sync
                        .get_syncer(&origin, &repo)
                        .sync_ref_spec(references_to_sync.try_into()?)
                        .await?;
                    for item in synced.ref_spec.iter() {
                        let digest = item.resolve_digest(&repo).await?;
                        runtime.push_digest(digest);
                    }
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
                    include_secondary_tags: runtime.config.include_secondary_tags,
                };
                let repo = spfs::storage::ProxyRepository::from_config(proxy_config)
                    .await
                    .wrap_err("Failed to build proxy repository for environment resolution")?;
                for item in reference.iter().filter(|i| !i.is_livelayer()) {
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
        // in cases where we are rerunning a persistent runtime,
        // the original editability should be preserved by default
        if self.edit {
            runtime.status.editable = true;
        } else if self.no_edit {
            runtime.status.editable = false;
        }
        runtime.save_state_to_storage().await?;

        tracing::debug!("resolving entry process");

        let mut cmd =
            spfs::build_command_for_runtime(runtime, command, self.command.drain(..).skip(1))?;

        let sync_time = start_time.elapsed();
        cmd.vars.push((
            "SPFS_METRICS_SYNC_TIME_SECS".into(),
            sync_time.as_secs_f64().to_string().into(),
        ));

        cmd.exec()
            .map(|_| 0)
            .wrap_err("Failed to execute runtime command")
    }
}
