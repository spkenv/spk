// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::future::ready;
use std::path::Path;
use std::pin::Pin;

use futures::{FutureExt, StreamExt, TryStreamExt};

use super::status::remount_runtime;
use crate::prelude::*;
use crate::tracking::{BlobHasher, BlobRead, ManifestBuilder, PathFilter};
use crate::{encoding, graph, runtime, storage, tracking, Error, Result};

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
        Ok(encoding::Digest::from_async_reader(reader).await?)
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
    repo: &'repo RepositoryHandle,
}

#[tonic::async_trait]
impl<'repo> BlobHasher for WriteToRepositoryBlobHasher<'repo> {
    async fn hash_blob(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<encoding::Digest> {
        self.repo.commit_blob(reader).await
    }
}

/// Manages the process of committing files to a repository
pub struct Committer<'repo, H = WriteToRepositoryBlobHasher<'repo>, F = ()>
where
    H: BlobHasher + Send + Sync,
    F: PathFilter + Send + Sync,
{
    repo: &'repo storage::RepositoryHandle,
    builder: ManifestBuilder<H, F>,
    max_concurrent_blobs: usize,
}

impl<'repo> Committer<'repo, WriteToRepositoryBlobHasher<'repo>, ()> {
    /// Create a new committer, with the default [`WriteToRepositoryBlobHasher`].
    pub fn new(repo: &'repo storage::RepositoryHandle) -> Self {
        let builder = ManifestBuilder::new().with_blob_hasher(WriteToRepositoryBlobHasher { repo });
        Self {
            repo,
            builder,
            max_concurrent_blobs: tracking::DEFAULT_MAX_CONCURRENT_BLOBS,
        }
    }
}

impl<'repo, H, F> Committer<'repo, H, F>
where
    H: BlobHasher + Send + Sync,
    F: PathFilter + Send + Sync,
{
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
    pub fn with_blob_hasher<H2>(self, hasher: H2) -> Committer<'repo, H2, F>
    where
        H2: BlobHasher + Send + Sync,
    {
        Committer {
            repo: self.repo,
            builder: self.builder.with_blob_hasher(hasher),
            max_concurrent_blobs: self.max_concurrent_blobs,
        }
    }

    /// Use this filter when committing files to storage.
    ///
    /// Only the changes/files matched by `filter` will be included.
    ///
    /// The filter is expected to match paths that are relative to the
    /// `$PREFIX` root, eg: `directory/filename` rather than
    /// `/spfs/directory/filename`.
    pub fn with_path_filter<F2>(self, filter: F2) -> Committer<'repo, H, F2>
    where
        F2: PathFilter + Send + Sync,
    {
        Committer {
            repo: self.repo,
            builder: self.builder.with_path_filter(filter),
            max_concurrent_blobs: self.max_concurrent_blobs,
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
        if manifest.is_empty() {
            return Err(Error::NothingToCommit);
        }
        let layer = self
            .repo
            .create_layer(&graph::Manifest::from(&manifest))
            .await?;
        runtime.push_digest(layer.digest()?);
        runtime.status.editable = false;
        runtime.save_state_to_storage().await?;
        remount_runtime(runtime).await?;
        Ok(layer)
    }

    /// Commit the full layer stack and working files to a new platform.
    pub async fn commit_platform(&self, runtime: &mut runtime::Runtime) -> Result<graph::Platform> {
        match self.commit_layer(runtime).await {
            Ok(_) | Err(Error::NothingToCommit) => (),
            Err(err) => return Err(err),
        }

        runtime.reload_state_from_storage().await?;
        if runtime.status.stack.is_empty() {
            Err(Error::NothingToCommit)
        } else {
            self.repo
                .create_platform(runtime.status.stack.clone())
                .await
        }
    }

    /// Commit a local file system directory to this storage.
    ///
    /// This collects all files to store as blobs and maintains a
    /// render of the manifest for use immediately.
    pub async fn commit_dir<P>(&self, path: P) -> Result<tracking::Manifest>
    where
        P: AsRef<Path>,
    {
        let path = tokio::fs::canonicalize(&path)
            .await
            .map_err(|err| Error::InvalidPath(path.as_ref().to_owned(), err))?;
        let manifest = {
            tracing::info!("building manifest");
            self.builder.compute_manifest(&path).await?
        };

        tracing::info!("committing manifest");
        let mut stream = futures::stream::iter(manifest.walk())
            .then(|node| {
                if !node.entry.kind.is_blob() {
                    return ready(ready(Ok(())).boxed());
                }
                let local_path = path.join(node.path.as_str());
                let entry = node.entry.clone();
                let fut = async move {
                    if self.repo.has_blob(entry.object).await {
                        return Ok(());
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
                            "File contents changed on disk during commit: {local_path:?}",
                        )));
                    }
                    Ok(())
                };
                ready(fut.boxed())
            })
            .buffer_unordered(self.max_concurrent_blobs)
            .boxed();
        while stream.try_next().await?.is_some() {}
        drop(stream);

        let storable = graph::Manifest::from(&manifest);
        self.repo
            .write_object(&graph::Object::Manifest(storable))
            .await?;

        Ok(manifest)
    }
}
