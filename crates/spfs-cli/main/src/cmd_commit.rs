// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;

use clap::Args;
use miette::Result;
use spfs::encoding::prelude::*;
use spfs::prelude::*;
use spfs_cli_common as cli;

/// Commit the current runtime state or a directory to storage
#[derive(Debug, Args)]
pub struct CmdCommit {
    /// Commit files directly into a remote repository
    ///
    /// The default is to commit to the local repository. This flag
    /// is only valid with the --path argument.
    #[clap(flatten)]
    pub(crate) repos: cli::Repositories,

    /// A human-readable tag for the generated object
    ///
    /// Can be provided more than once.
    #[clap(long = "tag", short)]
    tags: Vec<String>,

    /// Commit this directory instead of the current spfs changes
    #[clap(long)]
    path: Option<PathBuf>,

    /// Combine existing items into a platform, use a '+' to join multiple
    #[clap(long = "ref")]
    reference: Option<String>,

    /// Hash the files while committing, rather than before.
    ///
    /// This option can improve commit times when a large number of the
    /// files are both large, and don't already exist in the repository.
    /// It may degrade commit times when committing directly to a slow or
    /// remote repository. When given, all files are written to the
    /// repository even if the payload exists, rather than hashing
    /// the file first to determine if it needs to be transferred.
    #[clap(long)]
    hash_while_committing: bool,

    /// Deprecated, has no effect
    #[clap(long, hide = true)]
    hash_first: bool,

    /// The total number of blobs that can be committed concurrently
    #[clap(
        long,
        env = "SPFS_COMMIT_MAX_CONCURRENT_BLOBS",
        default_value_t = spfs::tracking::DEFAULT_MAX_CONCURRENT_BLOBS
    )]
    pub max_concurrent_blobs: usize,

    /// The total number of branches that can be processed concurrently
    /// at each level of the rendered file tree.
    ///
    /// The number of active trees being processed can grow exponentially
    /// by this exponent for each additional level of depth in the rendered
    /// file tree. In general, this number should be kept low.
    #[clap(
        long,
        env = "SPFS_COMMIT_MAX_CONCURRENT_BRANCHES",
        default_value_t = spfs::tracking::DEFAULT_MAX_CONCURRENT_BRANCHES
    )]
    pub max_concurrent_branches: usize,

    /// The desired object type to create, skip this when giving --path or --ref
    #[clap(
        value_parser = ["layer", "platform"],
        conflicts_with_all = &["path", "remote", "reference"],
        required_unless_present_any = &["path", "reference"]
    )]
    kind: Option<String>,

    /// Allow committing an empty layer or platform
    #[clap(long, hide = true)]
    allow_empty: bool,
}

impl CmdCommit {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo =
            spfs::config::open_repository_from_string(config, self.repos.remote.clone()).await?;

        let result = {
            let committer = spfs::Committer::new(&repo)
                .with_reporter(spfs::commit::ConsoleCommitReporter::default())
                .with_max_concurrent_branches(self.max_concurrent_branches)
                .with_max_concurrent_blobs(self.max_concurrent_blobs)
                .with_allow_empty(self.allow_empty);
            if self.hash_while_committing {
                let committer = committer
                    .with_blob_hasher(spfs::commit::WriteToRepositoryBlobHasher { repo: &repo });
                self.do_commit(&repo, committer).await?
            } else {
                self.do_commit(&repo, committer).await?
            }
        };

        tracing::info!(digest=%result.digest()?, "created");
        for tag in self.tags.iter() {
            let tag_spec = match spfs::tracking::TagSpec::parse(tag) {
                Ok(tag_spec) => tag_spec,
                Err(err) => {
                    tracing::warn!("cannot set invalid tag '{tag}': {err:?}");
                    continue;
                }
            };
            repo.push_tag(&tag_spec, &result.digest()?).await?;
            tracing::info!(?tag, "created");
        }
        if self.kind.is_some() {
            tracing::info!("edit mode disabled");
        }

        Ok(0)
    }

    async fn do_commit<'repo, H, F>(
        &self,
        repo: &'repo spfs::storage::RepositoryHandle,
        committer: spfs::Committer<'repo, H, F, spfs::commit::ConsoleCommitReporter>,
    ) -> spfs::Result<spfs::graph::Object>
    where
        H: spfs::tracking::BlobHasher + Send + Sync,
        F: spfs::tracking::PathFilter + Send + Sync,
    {
        if let Some(path) = &self.path {
            let manifest = committer.commit_dir(path).await?;
            if manifest.is_empty() && !self.allow_empty {
                return Err(spfs::runtime::Error::NothingToCommit.into());
            }
            return Ok(repo
                .create_layer(&manifest.to_graph_manifest())
                .await?
                .into());
        }

        if let Some(reference) = &self.reference {
            let env_spec = spfs::tracking::EnvSpec::parse(reference)?;
            let mut digests = spfs::graph::Stack::default();
            for env_item in env_spec.iter() {
                let env_item = env_item.to_string();
                let digest = repo.resolve_ref(env_item.as_ref()).await?;
                digests.push(digest);
            }

            return Ok(repo.create_platform(digests).await?.into());
        }

        // no path or reference given, commit the current runtime
        let mut runtime = spfs::active_runtime().await?;

        if !runtime.status.editable {
            return Err(spfs::Error::String(
                "Active runtime is not editable, nothing to commit".into(),
            ));
        }

        match self.kind.clone().unwrap_or_default().as_str() {
            "layer" => Ok(committer.commit_layer(&mut runtime).await?.into()),
            "platform" => Ok(committer.commit_platform(&mut runtime).await?.into()),
            kind => Err(spfs::Error::String(format!(
                "don't know how to commit a '{kind}', valid options are 'layer' and 'platform'"
            ))),
        }
    }
}
