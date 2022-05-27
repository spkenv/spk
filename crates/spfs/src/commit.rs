// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use super::config::get_config;
use super::status::remount_runtime;
use crate::tracking::ManifestBuilderHasher;
use crate::{encoding, graph, runtime, Error, Result};
use crate::{prelude::*, tracking};

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
    let path = tokio::fs::canonicalize(path).await?;
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
pub async fn commit_layer(runtime: &mut runtime::Runtime) -> Result<graph::Layer> {
    let config = get_config()?;
    let repo = Arc::new(config.get_repository().await?.into());
    let manifest = commit_dir(Arc::clone(&repo), runtime.upper_dir.as_path()).await?;
    if manifest.is_empty() {
        return Err(Error::NothingToCommit);
    }
    let layer = repo.create_layer(&graph::Manifest::from(&manifest)).await?;
    runtime.push_digest(&layer.digest()?)?;
    runtime.set_editable(false)?;
    remount_runtime(runtime).await?;
    Ok(layer)
}

/// Commit the full layer stack and working files to a new platform.
pub async fn commit_platform(runtime: &mut runtime::Runtime) -> Result<graph::Platform> {
    let config = get_config()?;
    let repo = config.get_repository().await?;

    match commit_layer(runtime).await {
        Ok(_) | Err(Error::NothingToCommit) => (),
        Err(err) => return Err(err),
    }

    let stack = runtime.get_stack();
    if stack.is_empty() {
        Err(Error::NothingToCommit)
    } else {
        repo.create_platform(stack.clone()).await
    }
}
