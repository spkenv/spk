// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::str::FromStr;
use std::time::Instant;

use clap::{Args, Subcommand};
use itertools::Itertools;
use miette::{Context, Result};
use spk_cli_common::{CommandArgs, Run, flags};
use spk_schema::VersionIdent;
use spk_storage::{self as storage, FlatBufferRepoIndex, RepositoryHandle, RepositoryIndexMut};
use storage::Repository;

/// Perform repository-level actions and maintenance
#[derive(Args)]
pub struct Repo {
    #[clap(subcommand)]
    command: RepoCommand,
}

#[async_trait::async_trait]
impl Run for Repo {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
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
    /// Generate an index for a repository
    Index {
        /// Repository to generate or update an index from.
        #[clap(long, short = 'r')]
        repo: String,

        /// Package or package/version of a published package to
        /// update in an existing index.
        ///
        /// Can be specified multiple times. Other packages in the
        /// index will not be updated. Without this the full index
        /// will be constructed from scratch. If the repo does not
        /// have an index, a full index will be constructed from
        /// scratch if the repository supports an index.
        ///
        /// This option is only supported for flatbuffer indexes.
        #[clap(long, name = "PACKAGE/VERSION")]
        update: Vec<String>,
    },
}

impl RepoCommand {
    pub async fn run(&mut self) -> Result<i32> {
        match &self {
            // spk repo upgrade ...
            Self::Upgrade { repo: repo_name } => {
                let repo = match repo_name.as_str() {
                    "local" => storage::local_repository().await?,
                    _ => storage::remote_repository(repo_name).await?,
                };

                let status = repo.upgrade().await.wrap_err("Upgrade failed")?;
                tracing::info!("{}", status);
                Ok(1)
            }

            // spk repo index ...
            Self::Index { repo, update } => {
                // Generate or update an index a repo. The repo must
                // be the underlying repo and not an indexed repo. So as
                // a safety measure, this disables index use for this
                // command regardless of config or command line flags.
                flags::disable_index_use();

                // Construct the repo handle to operate on, and repo
                // list that contains it.
                let repo_to_index: RepositoryHandle = match repo.as_str() {
                    "local" => storage::local_repository().await?.into(),
                    name => storage::remote_repository(name).await?.into(),
                };
                let repos = vec![(repo_to_index.name().to_string(), repo_to_index.clone())];

                if !update.is_empty() {
                    // Update the existing index for the given package/version
                    let start = Instant::now();
                    let idents: Vec<VersionIdent> = update
                        .iter()
                        .filter_map(|pv| match VersionIdent::from_str(pv) {
                            Ok(i) => Some(i),
                            Err(err) => {
                                tracing::warn!(
                                    "Skipping '{pv}': Unable to parse it as a package/version: {err}"
                                );
                                None
                            }
                        })
                        .collect();

                    tracing::debug!(
                        "Command line update option: [{}]",
                        update.iter().map(ToString::to_string).join(", ")
                    );
                    tracing::info!(
                        "Package/versions to update: [{}]",
                        idents.iter().map(ToString::to_string).join(", ")
                    );
                    if idents.is_empty() {
                        tracing::error!(
                            "No valid package/versions given, nothing to update. Stopping."
                        );
                        return Ok(2);
                    }

                    // Load the current index for this repo now
                    let mut was_full_index = String::from("");
                    match FlatBufferRepoIndex::from_repo_file(&repo_to_index).await {
                        Ok(current_index) => {
                            current_index
                                .update_packages(&repo_to_index, &idents)
                                .await?
                        }
                        Err(err) => {
                            // There isn't an existing index, so generate one from scratch that
                            // will also include the update package version.
                            tracing::warn!("Failed to load flatbuffer index: {err}");
                            tracing::warn!("No current index to update. Creating a full index ...");
                            FlatBufferRepoIndex::index_repo(&repos).await?;
                            was_full_index =
                                " [no previous index, so a full index was created]".to_string()
                        }
                    };

                    tracing::info!(
                        "Index update for '{}' in '{}' repo completed in: {} secs{was_full_index}",
                        idents.iter().map(ToString::to_string).join(", "),
                        repo_to_index.name(),
                        start.elapsed().as_secs_f64()
                    );
                } else {
                    // Generate a full index from scratch
                    let start = Instant::now();
                    FlatBufferRepoIndex::index_repo(&repos).await?;

                    tracing::info!(
                        "Index generation for '{}' repo completed in: {} secs",
                        repo_to_index.name(),
                        start.elapsed().as_secs_f64()
                    );
                }

                Ok(0)
            }
        }
    }
}
