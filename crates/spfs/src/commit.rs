// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::future::ready;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use futures::{FutureExt, StreamExt, TryStreamExt};
use once_cell::sync::OnceCell;
use progress_bar_derive_macro::ProgressBar;
use spfs_encoding::prelude::*;

use super::status::remount_runtime;
use crate::prelude::*;
use crate::tracking::{BlobHasher, BlobRead, ManifestBuilder, PathFilter};
use crate::{Error, Result, encoding, graph, runtime, storage, tracking};

#[cfg(test)]
#[path = "./commit_test.rs"]
mod commit_test;

/// Hashes blob data in-memory.
///
/// Used in conjunction with the [`Committer`], this hasher
/// can reduce io overhead when committing large and incremental
/// manifests, or when committing to a remote repository. It has
/// the benefit of not needing to write anything to the repository
/// that already exists, but in a worst-case scenario will require
/// reading the local files twice (once for hashing and once to copy
/// into the repository)
pub struct InMemoryBlobHasher;

#[tonic::async_trait]
impl BlobHasher for InMemoryBlobHasher {
    async fn hash_blob(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<encoding::Digest> {
        Ok(encoding::Hasher::hash_async_reader(reader).await?)
    }
}

/// Hashes payload data as it's being written to a repository.
///
/// Used in conjunction with the [`Committer`], this can reduce
/// io overhead by ensuring that each file only needs to be read
/// through once. It can also greatly decrease the commit speed
/// by requiring that each file is written to the repository even
/// if the payload already exists.
pub struct WriteToRepositoryBlobHasher<'repo> {
    pub repo: &'repo RepositoryHandle,
}

#[tonic::async_trait]
impl BlobHasher for WriteToRepositoryBlobHasher<'_> {
    async fn hash_blob(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<encoding::Digest> {
        self.repo.commit_blob(reader).await
    }
}

/// Manages the process of committing files to a repository
pub struct Committer<
    'repo,
    H = WriteToRepositoryBlobHasher<'repo>,
    F = (),
    Reporter = SilentCommitReporter,
> where
    H: BlobHasher + Send + Sync,
    F: PathFilter + Send + Sync,
    Reporter: CommitReporter,
{
    repo: &'repo storage::RepositoryHandle,
    reporter: Arc<Reporter>,
    builder: ManifestBuilder<H, F, Arc<Reporter>>,
    max_concurrent_blobs: usize,
    allow_empty: bool,
}

impl<'repo> Committer<'repo, InMemoryBlobHasher, (), SilentCommitReporter> {
    /// Create a new committer, with the default [`InMemoryBlobHasher`].
    pub fn new(repo: &'repo storage::RepositoryHandle) -> Self {
        let reporter = Arc::new(SilentCommitReporter);
        let builder = ManifestBuilder::new()
            .with_blob_hasher(InMemoryBlobHasher)
            .with_reporter(Arc::clone(&reporter));
        Self {
            repo,
            reporter,
            builder,
            max_concurrent_blobs: tracking::DEFAULT_MAX_CONCURRENT_BLOBS,
            allow_empty: false,
        }
    }
}

impl<'repo, H, F, R> Committer<'repo, H, F, R>
where
    H: BlobHasher + Send + Sync,
    F: PathFilter + Send + Sync,
    R: CommitReporter,
{
    /// Set if an empty commit is allowed.
    ///
    /// Defaults to false.
    pub fn with_allow_empty(mut self, allow_empty: bool) -> Self {
        self.allow_empty = allow_empty;
        self
    }

    /// Set how many blobs should be processed at once.
    ///
    /// Defaults to [`tracking::DEFAULT_MAX_CONCURRENT_BLOBS`].
    pub fn with_max_concurrent_blobs(mut self, max_concurrent_blobs: usize) -> Self {
        self.builder = self.builder.with_max_concurrent_blobs(max_concurrent_blobs);
        self.max_concurrent_blobs = max_concurrent_blobs;
        self
    }

    /// Set how many branches should be processed at once (during manifest building).
    ///
    /// Each tree/folder that is processed can have any number of subtrees. This number
    /// limits the number of subtrees that can be processed at once for any given tree. This
    /// means that the number compounds exponentially based on the depth of the manifest
    /// being computed. Eg: a limit of 2 allows two directories to be processed in the root
    /// simultaneously and a further 2 within each of those two for a total of 4 branches, and so
    /// on. When computing for extremely deep trees, a smaller, conservative number is better
    /// to avoid open file limits.
    pub fn with_max_concurrent_branches(mut self, max_concurrent_branches: usize) -> Self {
        self.builder = self
            .builder
            .with_max_concurrent_branches(max_concurrent_branches);
        self
    }

    /// Use the given [`BlobHasher`] when building the manifest.
    ///
    /// See [`InMemoryBlobHasher`] and [`WriteToRepositoryBlobHasher`] for
    /// details on different strategies that can be employed when committing.
    pub fn with_blob_hasher<H2>(self, hasher: H2) -> Committer<'repo, H2, F, R>
    where
        H2: BlobHasher + Send + Sync,
    {
        Committer {
            repo: self.repo,
            builder: self.builder.with_blob_hasher(hasher),
            reporter: self.reporter,
            max_concurrent_blobs: self.max_concurrent_blobs,
            allow_empty: self.allow_empty,
        }
    }

    /// Use the given [`CommitReporter`] when running, replacing any existing one.
    pub fn with_reporter<R2>(self, reporter: impl Into<Arc<R2>>) -> Committer<'repo, H, F, R2>
    where
        R2: CommitReporter,
    {
        let reporter = reporter.into();
        Committer {
            repo: self.repo,
            builder: self.builder.with_reporter(Arc::clone(&reporter)),
            reporter,
            max_concurrent_blobs: self.max_concurrent_blobs,
            allow_empty: self.allow_empty,
        }
    }

    /// Use this filter when committing files to storage.
    ///
    /// Only the changes/files matched by `filter` will be included.
    ///
    /// The filter is expected to match paths that are relative to the
    /// `$PREFIX` root, eg: `directory/filename` rather than
    /// `/spfs/directory/filename`.
    pub fn with_path_filter<F2>(self, filter: F2) -> Committer<'repo, H, F2, R>
    where
        F2: PathFilter + Send + Sync,
    {
        Committer {
            repo: self.repo,
            builder: self.builder.with_path_filter(filter),
            reporter: self.reporter,
            max_concurrent_blobs: self.max_concurrent_blobs,
            allow_empty: self.allow_empty,
        }
    }

    /// Commit the working file changes of a runtime to a new layer.
    pub async fn commit_layer(&self, runtime: &mut runtime::Runtime) -> Result<graph::Layer> {
        let manifest = self.commit_dir(&runtime.config.upper_dir).await?;
        self.commit_manifest(manifest, runtime).await
    }

    /// Commit a manifest of the working file changes of a runtime to a new layer.
    ///
    /// This will add the layer to the current runtime and then remount it.
    pub async fn commit_manifest(
        &self,
        manifest: tracking::Manifest,
        runtime: &mut runtime::Runtime,
    ) -> Result<graph::Layer> {
        if manifest.is_empty() && !self.allow_empty {
            return Err(Error::NothingToCommit);
        }
        let layer = self
            .repo
            .create_layer(&manifest.to_graph_manifest())
            .await?;
        if !manifest.is_empty() {
            // Don't bother putting the empty layer on the stack, the goal
            // with allow_empty is to create an empty manifest.
            if !runtime.push_digest(layer.digest()?) {
                return Err(Error::NothingToCommit);
            }
            runtime.status.editable = false;
            runtime.save_state_to_storage().await?;
            remount_runtime(runtime).await?;
        }
        Ok(layer)
    }

    /// Commit the full layer stack and working files to a new platform.
    pub async fn commit_platform(&self, runtime: &mut runtime::Runtime) -> Result<graph::Platform> {
        match self.commit_layer(runtime).await {
            Ok(_) | Err(Error::NothingToCommit) => (),
            Err(err) => return Err(err),
        }

        runtime.reload_state_from_storage().await?;
        if runtime.status.stack.is_empty() && !self.allow_empty {
            Err(Error::NothingToCommit)
        } else {
            self.repo
                .create_platform(runtime.status.stack.clone())
                .await
        }
    }

    /// Calculate the manifest for the given path.
    ///
    /// Returns a tuple of the canonicalized path and its
    /// [`tracking::Manifest`].
    pub async fn manifest_for_path<P>(&self, path: P) -> Result<(PathBuf, tracking::Manifest)>
    where
        P: AsRef<Path>,
    {
        let path = dunce::canonicalize(&path)
            .map_err(|err| Error::InvalidPath(path.as_ref().to_owned(), err))?;
        let manifest = self.builder.compute_manifest(&path).await?;
        Ok((path, manifest))
    }

    /// Commit a local file system directory to this storage.
    ///
    /// This collects all files to store as blobs and maintains a
    /// render of the manifest for use immediately.
    pub async fn commit_dir<P>(&self, path: P) -> Result<tracking::Manifest>
    where
        P: AsRef<Path>,
    {
        let (path, manifest) = self.manifest_for_path(&path).await?;

        let mut stream = futures::stream::iter(manifest.walk_abs("."))
            .filter_map(|node| {
                if !node.entry.kind.is_blob() {
                    return ready(None);
                }
                let relative_path = std::path::Path::new(node.path.as_str());
                self.reporter.visit_blob(&node);
                let local_path = path.join(relative_path);
                let node = node.into_owned();
                let fut = async move {
                    let entry = &node.entry;
                    let (has_object, has_payload) = tokio::join!(
                        self.repo.has_object(entry.object),
                        self.repo.has_payload(entry.object),
                    );
                    if has_object && has_payload {
                        return Ok(CommitBlobResult::AlreadyExists(node));
                    }
                    let created = if entry.is_symlink() {
                        let content = tokio::fs::read_link(&local_path)
                            .await
                            .map_err(|err| {
                                // TODO: add better message for file missing
                                Error::StorageWriteError(
                                    "read link for committing",
                                    local_path.clone(),
                                    err,
                                )
                            })?
                            .into_os_string()
                            .into_string()
                            .map_err(|_| {
                                crate::Error::String(
                                    "Symlinks must point to a valid utf-8 path".to_string(),
                                )
                            })?
                            .into_bytes();
                        let reader =
                            Box::pin(tokio::io::BufReader::new(std::io::Cursor::new(content)));
                        self.repo.commit_blob(reader).await?
                    } else {
                        let file = tokio::fs::File::open(&local_path).await.map_err(|err| {
                            // TODO: add better message for file missing
                            Error::StorageWriteError(
                                "open file for committing",
                                local_path.clone(),
                                err,
                            )
                        })?;
                        let reader = Box::pin(tokio::io::BufReader::new(file));
                        self.repo.commit_blob(reader).await?
                    };
                    if created != entry.object {
                        return Err(Error::String(format!(
                            "File contents changed on disk during commit: {local_path:?} [{created} != {}", entry.object
                        )));
                    }
                    Ok(CommitBlobResult::Committed(node))
                };
                ready(Some(fut.boxed()))
            })
            .buffer_unordered(self.max_concurrent_blobs)
            .boxed();
        while let Some(result) = stream.try_next().await? {
            self.reporter.committed_blob(&result);
        }
        drop(stream);

        let storable = manifest.to_graph_manifest();
        self.repo.write_object(&storable).await?;

        Ok(manifest)
    }
}

/// The result of committing a single file from a manifest
pub enum CommitBlobResult {
    /// The blob was written to the repository
    Committed(tracking::OwnedManifestNode),
    /// The associated blob already exists in the repository so
    /// nothing needed to be done
    AlreadyExists(tracking::OwnedManifestNode),
}

impl CommitBlobResult {
    pub fn node(&self) -> &tracking::OwnedManifestNode {
        match self {
            CommitBlobResult::Committed(n) => n,
            CommitBlobResult::AlreadyExists(n) => n,
        }
    }
}

/// Receives updates from a sync process to be reported.
///
/// Unless the sync runs into errors, every call to visit_* is
/// followed up by a call to the corresponding synced_*.
pub trait CommitReporter: tracking::ComputeManifestReporter + Send + Sync {
    /// Called when a manifest node is being committed to the repository
    ///
    /// This node is guaranteed to be a blob, and may turn out to be a no-op
    /// if the blob and payload already exist in the target repository
    fn visit_blob(&self, _node: &tracking::ManifestNode) {}

    /// Called after a blob has been committed to the repository
    fn committed_blob(&self, _result: &CommitBlobResult) {}
}

/// A reporter for use in the commit process that drops all events
#[derive(Default)]
pub struct SilentCommitReporter;
impl tracking::ComputeManifestReporter for SilentCommitReporter {}
impl CommitReporter for SilentCommitReporter {}

/// Reports commit progress to an interactive console via progress bars
#[derive(Default)]
pub struct ConsoleCommitReporter {
    bars: OnceCell<ConsoleCommitReporterBars>,
}

impl ConsoleCommitReporter {
    fn get_bars(&self) -> &ConsoleCommitReporterBars {
        self.bars.get_or_init(Default::default)
    }
}

impl tracking::ComputeManifestReporter for ConsoleCommitReporter {
    fn visit_entry(&self, _path: &std::path::Path) {
        let bars = self.get_bars();
        bars.entries.inc_length(1);
    }

    fn computed_entry(&self, entry: &tracking::Entry) {
        let bars = self.get_bars();
        bars.entries.inc(1);
        if entry.kind.is_blob() {
            bars.blobs.inc_length(1);
            bars.bytes.inc_length(entry.size());
        }
    }
}

impl CommitReporter for ConsoleCommitReporter {
    fn committed_blob(&self, result: &CommitBlobResult) {
        let bars = self.get_bars();
        if result.node().entry.kind.is_blob() {
            bars.bytes.inc(result.node().entry.size());
            bars.blobs.inc(1);
        } else {
            debug_assert!(false, "committed_blob called with non-blob entry");
            bars.blobs.inc(1);
        }
    }
}

#[derive(ProgressBar)]
#[progress_bar(template = "      {spinner} {msg:<18.green} [{bar:40.cyan/dim}] {pos:>8}/{len:6}")]
struct ConsoleCommitReporterBars {
    #[progress_bar(message = "computing manifest")]
    entries: indicatif::ProgressBar,
    #[progress_bar(message = "committing blobs")]
    blobs: indicatif::ProgressBar,
    #[progress_bar(
        message = "committing data",
        template = "      {spinner} {msg:<18.green} [{bar:40.cyan/dim}] {bytes:>8}/{total_bytes:7}"
    )]
    bytes: indicatif::ProgressBar,
}
