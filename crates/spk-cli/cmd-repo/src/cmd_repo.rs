// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::str::FromStr;
use std::time::Instant;

use clap::{Args, Subcommand};
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
        /// Repositories to enable for the command
        ///
        /// Any configured spfs repository can be named here as well as "local" or
        /// a path on disk or a full remote repository url. Repositories can also
        /// be limited to a specific time by appending a relative or absolute time
        /// specifier (eg: origin~10m, origin~5weeks, origin@2022-10-11,
        /// origin@2022-10-11T13:00.12). This time affects all interactions and
        /// queries in the repository, effectively making it look like it did in the past.
        /// It will cause errors for any operation that attempts to make changes to
        /// the repository, even if the time is in the future.
        #[clap(long, short = 'r')]
        repo: String,

        /// Package/version of a published package to update in an
        /// existing index.
        ///
        /// Other packages in the index will remain unchanged. Without
        /// this the full index will be constructed from scratch. If
        /// the repo does not have an index, a full index will be
        /// constructed from scratch if possible.
        ///
        /// This option is only supported for flatbuffer indexes.
        #[clap(long, name = "PACKAGE/VERSION")]
        update: Option<String>,
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

                if let Some(package_version) = update {
                    // Update the existing index for the given package/version
                    let start = Instant::now();
                    let version_ident = VersionIdent::from_str(package_version)?;
                    let mut was_full_index = String::from("");

                    // Load the current index for this repo now
                    match FlatBufferRepoIndex::from_repo_file(&repo_to_index).await {
                        Ok(current_index) => {
                            current_index
                                .update_repo_with_package_version(&repo_to_index, &version_ident)
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
                        "Index update for '{package_version}' in '{}' repo completed in: {} secs{was_full_index}",
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
