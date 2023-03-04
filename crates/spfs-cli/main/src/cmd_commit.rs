// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Args;
use spfs::encoding::Encodable;

/// Commit the current runtime state or a directory to storage
#[derive(Debug, Args)]
pub struct CmdCommit {
    /// Commit files directly into a remote repository
    ///
    /// The default is to commit to the local repository. This flag
    /// is only valid with the --path argument.
    #[clap(long, short)]
    remote: Option<String>,

    /// A human-readable tag for the generated object
    ///
    /// Can be provided more than once.
    #[clap(long = "tag", short)]
    tags: Vec<String>,

    /// Commit this directory instead of the current spfs changes
    #[clap(long)]
    path: Option<PathBuf>,

    /// Hash the files before committing, rather than while committing.
    ///
    /// This option can improve commit times when a large number of the
    /// files are expected to already exist in the repository. It can
    /// also improve commit times when committing directly to a slow or
    /// remote repository. By default, all files are written to the
    /// repository even if the payload exists, but this strategy will
    /// hash the file first to determine if it needs to be transferred.
    #[clap(long)]
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

    /// The desired object type to create, skip this when giving --path
    #[clap(
        possible_values = &["layer", "platform"],
        conflicts_with_all = &["path", "remote"],
        required_unless_present = "path",
    )]
    kind: Option<String>,
}

impl CmdCommit {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo = Arc::new(
            config
                .get_remote_repository_or_local(self.remote.as_ref())
                .await?,
        );

        let committer = spfs::Committer::new(&repo)
            .with_max_concurrent_branches(self.max_concurrent_branches)
            .with_max_concurrent_blobs(self.max_concurrent_blobs);
        let result = if self.hash_first {
            let committer = committer.with_blob_hasher(spfs::commit::InMemoryBlobHasher);
            self.do_commit(&repo, committer).await?
        } else {
            self.do_commit(&repo, committer).await?
        };

        tracing::info!(digest = ?result.digest()?, "created");
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
        committer: spfs::Committer<'repo, H, F>,
    ) -> spfs::Result<spfs::graph::Object>
    where
        H: spfs::tracking::BlobHasher + Send + Sync,
        F: spfs::tracking::PathFilter + Send + Sync,
    {
        if let Some(path) = &self.path {
            let manifest = committer.commit_dir(path).await?;
            if manifest.is_empty() {
                return Err(spfs::Error::NothingToCommit);
            }
            return Ok(repo
                .create_layer(&spfs::graph::Manifest::from(&manifest))
                .await?
                .into());
        }
        // no path given, commit the current runtime

        let mut runtime = spfs::active_runtime().await?;

        if !runtime.status.editable {
            return Err(spfs::Error::String(
                "Active runtime is not editable, nothing to commit".into(),
            ));
        }

        match self.kind.clone().unwrap_or_default().as_str() {
            "layer" => Ok(committer.commit_layer(&mut runtime).await?.into()),
            "platform" => Ok(committer.commit_platform(&mut runtime).await?.into()),
            kind => {
                return Err(spfs::Error::String(format!(
                    "don't know how to commit a '{kind}'"
                )));
            }
        }
    }
}
