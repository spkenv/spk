// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;
use std::pin::Pin;

use async_trait::async_trait;
use encoding::prelude::*;
use tokio_stream::StreamExt;

use super::OpenRepositoryResult;
use super::fs::{FsHashStore, RenderStore, RenderStoreCreationPolicy};
use crate::tracking::{self, BlobRead};
use crate::{Error, Result, encoding, graph};

#[cfg(test)]
#[path = "./repository_test.rs"]
mod repository_test;

#[cfg(test)]
#[path = "./database_test.rs"]
mod database_test;

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum Ref {
    Digest(encoding::Digest),
    TagSpec(tracking::TagSpec),
}

impl std::fmt::Display for Ref {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ref::Digest(d) => write!(f, "{d}"),
            Ref::TagSpec(t) => write!(f, "{t}"),
        }
    }
}

/// Represents a storage location for spfs data.
#[async_trait]
pub trait Repository:
    super::Address
    + super::TagStorage
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
    /// Return true if this repository contains the given reference.
    async fn has_ref(&self, reference: &str) -> bool {
        self.read_ref(reference).await.is_ok()
    }

    /// Resolve a tag or digest string into its absolute digest.
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
        self.read_object(digest).await
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
}

/// Blanket implementation.
impl<T> Repository for T where
    T: super::Address
        + super::TagStorage
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
}

#[async_trait]
pub trait RepositoryExt: super::PayloadStorage + graph::DatabaseExt {
    /// Commit the data from 'reader' as a blob in this repository
    async fn commit_blob(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<encoding::Digest> {
        // Safety: it is unsafe to write data without also creating a blob
        // to track that payload, which is exactly what this function is doing
        let (digest, size) = unsafe { self.write_data(reader).await? };
        let blob = graph::Blob::new(digest, size);
        self.write_object(&blob).await?;
        Ok(digest)
    }
}

/// Blanket implementation.
impl<T> RepositoryExt for T where T: super::PayloadStorage + graph::DatabaseExt {}

/// Accessor methods for types only applicable to repositories that have
/// payloads, e.g., local repositories.
pub trait LocalPayloads {
    /// Return the payload storage type
    fn payloads(&self) -> &FsHashStore;
}

/// A trait for types that can support render stores, this provides a way to
/// instantiate those types.
pub trait RenderStoreForUser {
    type RenderStore;

    /// Create an instance of the render store for the given user.
    ///
    /// This doesn't necessarily create the render store on disk; it depends on
    /// the implementation.
    ///
    /// The `url` parameter is the URL of the repository the render store
    /// belongs to.
    fn render_store_for_user(
        creation_policy: RenderStoreCreationPolicy,
        url: url::Url,
        root: &Path,
        username: &Path,
    ) -> OpenRepositoryResult<Self::RenderStore>;
}

/// A trait for types that might have render store, but no guarantees are made
/// that it exists or is accessible.
pub trait TryRenderStore {
    /// Return the render store for repositories that support it.
    ///
    /// For some types this may create the render store for the user on demand
    /// and may fail.
    fn try_render_store(&self) -> OpenRepositoryResult<Cow<'_, RenderStore>>;
}

/// Accessor methods for types only applicable to repositories that have
/// renders, e.g., local repositories.
pub trait LocalRenderStore: RenderStoreForUser {
    /// Returns the type responsible for locally rendered manifests
    fn render_store(&self) -> &RenderStore;
}
