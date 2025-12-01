// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;
use std::pin::Pin;

use async_trait::async_trait;
use encoding::prelude::*;
use tokio_stream::StreamExt;

use super::fs::{FsHashStore, RenderStore};
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
    + super::LayerStorage
    + super::PlatformStorage
    + graph::Database
    + graph::DatabaseView
    + std::fmt::Debug
    + Send
    + Sync
{
    /// Return true if this repository contains the given reference.
    ///
    /// This does not work for payload digests.
    async fn has_ref(&self, reference: &str) -> bool {
        self.read_ref(reference).await.is_ok()
    }

    /// Resolve a tag or digest string into its absolute digest.
    async fn resolve_ref(&self, reference: &str) -> Result<encoding::Digest> {
        if let Ok(tag_spec) = tracking::TagSpec::parse(reference)
            && let Ok(tag) = self.resolve_tag(&tag_spec).await
        {
            return Ok(tag.target);
        }

        let partial = encoding::PartialDigest::parse(reference)
            .map_err(|_| Error::UnknownReference(reference.to_string()))?;
        // This will discover the type of item but discard it. Do callers want
        // this information? A new type could be added to wrap FoundDigest with
        // a variant that doesn't have the type information. Note how resolving
        // a tag above does not determine the type, or require the item exists.
        self.resolve_full_digest(&partial)
            .await
            .map(|found_digest| found_digest.into_digest())
    }

    /// Read an object of unknown type by tag or digest.
    ///
    /// This does not work for payload digests.
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
    /// Commit the data from 'reader' as a payload in this repository
    async fn commit_payload(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<encoding::Digest> {
        let (digest, _size) = self.write_data(reader).await?;
        Ok(digest)
    }
}

/// Blanket implementation.
impl<T> RepositoryExt for T where T: super::PayloadStorage + graph::DatabaseExt {}

/// Accessor methods for types only applicable to repositories that have
/// payloads and renders, e.g., local repositories.
pub trait LocalRepository {
    /// Return the payload storage type
    fn payloads(&self) -> &FsHashStore;

    /// If supported, returns the type responsible for locally rendered manifests
    ///
    /// # Errors:
    /// - [`Error::NoRenderStorage`] - if this repository does not support manifest rendering
    fn render_store(&self) -> Result<&RenderStore>;
}
