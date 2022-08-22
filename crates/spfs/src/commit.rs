// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use super::status::remount_runtime;
use crate::tracking::ManifestBuilderHasher;
use crate::{encoding, graph, prelude::*, runtime, tracking, Error, Result};

#[cfg(test)]
#[path = "./commit_test.rs"]
mod commit_test;

struct CommitBlobHasher {
    repo: Arc<RepositoryHandle>,
}

#[tonic::async_trait]
impl ManifestBuilderHasher for CommitBlobHasher {
    async fn hasher(
        &self,
        reader: Pin<Box<dyn tokio::io::AsyncRead + Send + Sync + 'static>>,
    ) -> Result<encoding::Digest> {
        self.repo.commit_blob(reader).await
    }
}

/// Commit a local file system directory to this storage.
///
/// This collects all files to store as blobs and maintains a
/// render of the manifest for use immediately.
pub async fn commit_dir<P>(repo: Arc<RepositoryHandle>, path: P) -> Result<tracking::Manifest>
where
    P: AsRef<Path>,
{
    let path = tokio::fs::canonicalize(&path)
        .await
        .map_err(|err| Error::InvalidPath(path.as_ref().to_owned(), err))?;
    let manifest = {
        let builder = tracking::ManifestBuilder::new(CommitBlobHasher {
            repo: Arc::clone(&repo),
        });
        tracing::info!("committing files");
        builder.compute_manifest(path).await?
    };

    tracing::info!("writing manifest");
    let storable = graph::Manifest::from(&manifest);
    repo.write_object(&graph::Object::Manifest(storable))
        .await?;
    for node in manifest.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        let blob = graph::Blob::new(node.entry.object, node.entry.size);
        repo.write_object(&graph::Object::Blob(blob)).await?;
    }

    Ok(manifest)
}

/// Commit the working file changes of a runtime to a new layer.
pub async fn commit_layer(
    runtime: &mut runtime::Runtime,
    repo: Arc<RepositoryHandle>,
) -> Result<graph::Layer> {
    let manifest = commit_dir(Arc::clone(&repo), &runtime.config.upper_dir).await?;
    if manifest.is_empty() {
        return Err(Error::NothingToCommit);
    }
    let layer = repo.create_layer(&graph::Manifest::from(&manifest)).await?;
    runtime.push_digest(layer.digest()?);
    runtime.status.editable = false;
    runtime.save_state_to_storage().await?;
    remount_runtime(runtime).await?;
    Ok(layer)
}

/// Commit the full layer stack and working files to a new platform.
pub async fn commit_platform(
    runtime: &mut runtime::Runtime,
    repo: Arc<RepositoryHandle>,
) -> Result<graph::Platform> {
    match commit_layer(runtime, Arc::clone(&repo)).await {
        Ok(_) | Err(Error::NothingToCommit) => (),
        Err(err) => return Err(err),
    }

    runtime.reload_state_from_storage().await?;
    if runtime.status.stack.is_empty() {
        Err(Error::NothingToCommit)
    } else {
        repo.create_platform(runtime.status.stack.clone()).await
    }
}
