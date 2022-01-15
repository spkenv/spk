// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashSet;

use async_trait::async_trait;
use tokio_stream::StreamExt;

use super::ManifestViewer;
use crate::{encoding, graph, tracking, Error, Result};
use encoding::Encodable;
use graph::{Blob, Manifest};

#[cfg(test)]
#[path = "./repository_test.rs"]
mod repository_test;

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum Ref {
    Digest(encoding::Digest),
    TagSpec(tracking::TagSpec),
}

impl std::string::ToString for Ref {
    fn to_string(&self) -> String {
        match self {
            Self::Digest(d) => d.to_string(),
            Self::TagSpec(t) => t.to_string(),
        }
    }
}

/// Represents a storage location for spfs data.
#[async_trait]
pub trait Repository:
    super::TagStorage
    + super::PayloadStorage
    + super::ManifestStorage
    + super::BlobStorage
    + super::LayerStorage
    + super::PlatformStorage
    + graph::Database
    + graph::DatabaseView
    + std::fmt::Debug
    + Send
    + Sync
{
    /// Return the address of this repository.
    fn address(&self) -> url::Url;

    /// If supported, returns the type responsible for locally rendered manifests
    ///
    /// # Errors:
    /// - [`NoRenderStorage`] - if this repository does not support manifest rendering
    fn renders(&self) -> Result<Box<dyn ManifestViewer>> {
        Err(Error::NoRenderStorage(self.address()))
    }

    /// Return true if this repository contains the given reference.
    async fn has_ref(&self, reference: &str) -> bool {
        self.read_ref(reference).await.is_ok()
    }

    /// Resolve a tag or digest string into it's absolute digest.
    async fn resolve_ref(&self, reference: &str) -> Result<encoding::Digest> {
        if let Ok(tag_spec) = tracking::TagSpec::parse(reference) {
            if let Ok(tag) = self.resolve_tag(&tag_spec).await {
                return Ok(tag.target);
            }
        }

        let partial = encoding::PartialDigest::parse(reference)
            .map_err(|_| Error::UnknownReference(reference.to_string()))?;
        self.resolve_full_digest(&partial).await
    }

    /// Read an object of unknown type by tag or digest.
    async fn read_ref(&self, reference: &str) -> Result<graph::Object> {
        let digest = self.resolve_ref(reference).await?;
        self.read_object(&digest).await
    }

    /// Return the other identifiers that can be used for 'reference'.
    async fn find_aliases(&self, reference: &str) -> Result<HashSet<Ref>> {
        let mut aliases = HashSet::new();
        let digest = self.read_ref(reference).await?.digest()?;
        let mut tags = self.find_tags(&digest);
        while let Some(spec) = tags.next().await {
            aliases.insert(Ref::TagSpec(spec?));
        }
        if reference != digest.to_string().as_str() {
            aliases.insert(Ref::Digest(digest));
        }
        let mut dupe = None;
        for alias in aliases.iter().collect::<Vec<_>>() {
            if alias.to_string().as_str() == reference {
                dupe = Some(alias.clone());
                break;
            }
        }
        if let Some(r) = dupe {
            aliases.remove(&r);
        }
        Ok(aliases)
    }

    /// Commit the data from 'reader' as a blob in this repository
    async fn commit_blob(
        &mut self,
        reader: Box<dyn std::io::Read + Send + 'static>,
    ) -> Result<encoding::Digest> {
        let (digest, size) = self.write_data(reader).await?;
        let blob = Blob::new(digest, size);
        self.write_object(&graph::Object::Blob(blob)).await?;
        Ok(digest)
    }

    /// Commit a local file system directory to this storage.
    ///
    /// This collects all files to store as blobs and maintains a
    /// render of the manifest for use immediately.
    async fn commit_dir(&mut self, path: &std::path::Path) -> Result<tracking::Manifest> {
        let path = std::fs::canonicalize(path)?;
        // NOTE(rbottriell): I tried many different ways to define and structure
        // the manifest builder in order to avoid these additional sync primitives
        // but this is the best that I could come up with after all... basically
        // we need to wrap self so that it can be safely sent and shared with
        // the manifest buidler but then still be able to use it after the build
        // is finished. Overall, I'm still suspicious that there is a cleaner
        // way to get this to work properly since self is already bound to sync + send
        let repo = std::sync::Arc::new(tokio::sync::Mutex::new(self));
        let manifest = {
            let mut builder = tracking::ManifestBuilder::new(|reader| async {
                repo.lock().await.commit_blob(reader).await
            });
            tracing::info!("committing files");
            builder.compute_manifest(path).await?
        };

        let mut slf = repo.lock().await;
        tracing::info!("writing manifest");
        let storable = Manifest::from(&manifest);
        slf.write_object(&graph::Object::Manifest(storable)).await?;
        for node in manifest.walk() {
            if !node.entry.kind.is_blob() {
                continue;
            }
            let blob = Blob::new(node.entry.object, node.entry.size);
            slf.write_object(&graph::Object::Blob(blob)).await?;
        }

        Ok(manifest)
    }
}

impl<T: Repository> Repository for &mut T {
    fn address(&self) -> url::Url {
        Repository::address(&**self)
    }
}
