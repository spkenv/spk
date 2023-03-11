// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashSet;
use std::pin::Pin;

use async_trait::async_trait;
use encoding::Encodable;
use graph::Blob;
use tokio_stream::StreamExt;

use super::fs::{FSHashStore, RenderStore};
use crate::{encoding, graph, tracking, Error, Result};

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

    /// Commit the data from 'reader' as a blob in this repository
    async fn commit_blob(
        &self,
        reader: Pin<Box<dyn tokio::io::AsyncBufRead + Send + Sync + 'static>>,
    ) -> Result<encoding::Digest> {
        // Safety: it is unsafe to write data without also creating a blob
        // to track that payload, which is exactly what this function is doing
        let (digest, size) = unsafe { self.write_data(reader).await? };
        let blob = Blob::new(digest, size);
        self.write_object(&graph::Object::Blob(blob)).await?;
        Ok(digest)
    }
}

impl<T: Repository> Repository for &T {
    fn address(&self) -> url::Url {
        Repository::address(&**self)
    }
}

/// Accessor methods for types only applicable to repositories that have
/// payloads and renders, e.g., local repositories.
pub trait LocalRepository {
    /// Return the payload storage type
    fn payloads(&self) -> &FSHashStore;

    /// If supported, returns the type responsible for locally rendered manifests
    ///
    /// # Errors:
    /// - [`Error::NoRenderStorage`] - if this repository does not support manifest rendering
    fn render_store(&self) -> Result<&RenderStore>;
}
