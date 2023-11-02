// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::Stream;
use relative_path::RelativePath;
use spfs_encoding as encoding;

use super::prelude::*;
use super::repository::Ref;
use super::tag::TagSpecAndTagStream;
use super::RepositoryHandle;
use crate::tracking::{self, BlobRead};
use crate::{graph, Result};

/// Runs a code block on each variant of the handle,
/// easily allowing the use of storage code without using
/// a dyn reference
macro_rules! each_variant {
    ($repo:expr, $inner:ident, $ops:tt) => {
        match $repo {
            RepositoryHandle::FS($inner) => $ops,
            RepositoryHandle::Tar($inner) => $ops,
            RepositoryHandle::Rpc($inner) => $ops,
            RepositoryHandle::FallbackProxy($inner) => $ops,
            RepositoryHandle::Proxy($inner) => $ops,
            RepositoryHandle::Pinned($inner) => $ops,
        }
    };
}

#[async_trait::async_trait]
impl Repository for RepositoryHandle {
    fn address(&self) -> url::Url {
        each_variant!(self, repo, { repo.address() })
    }

    async fn has_ref(&self, reference: &str) -> bool {
        each_variant!(self, repo, { repo.has_ref(reference).await })
    }

    async fn resolve_ref(&self, reference: &str) -> Result<encoding::Digest> {
        each_variant!(self, repo, { repo.resolve_ref(reference).await })
    }

    async fn read_ref(&self, reference: &str) -> Result<graph::Object> {
        each_variant!(self, repo, { repo.read_ref(reference).await })
    }

    async fn find_aliases(&self, reference: &str) -> Result<HashSet<Ref>> {
        each_variant!(self, repo, { repo.find_aliases(reference).await })
    }

    async fn commit_blob(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<encoding::Digest> {
        each_variant!(self, repo, { repo.commit_blob(reader).await })
    }
}

#[async_trait::async_trait]
impl TagStorage for RepositoryHandle {
    async fn resolve_tag(&self, tag_spec: &tracking::TagSpec) -> Result<tracking::Tag> {
        each_variant!(self, repo, { repo.resolve_tag(tag_spec).await })
    }

    fn ls_tags(
        &self,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<super::EntryType>> + Send>> {
        each_variant!(self, repo, { repo.ls_tags(path) })
    }

    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        each_variant!(self, repo, { repo.find_tags(digest) })
    }

    fn iter_tag_streams(&self) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        each_variant!(self, repo, { repo.iter_tag_streams() })
    }

    async fn read_tag(
        &self,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        each_variant!(self, repo, { repo.read_tag(tag).await })
    }

    async fn insert_tag(&self, tag: &tracking::Tag) -> Result<()> {
        each_variant!(self, repo, { repo.insert_tag(tag).await })
    }

    async fn remove_tag_stream(&self, tag: &tracking::TagSpec) -> Result<()> {
        each_variant!(self, repo, { repo.remove_tag_stream(tag).await })
    }

    async fn remove_tag(&self, tag: &tracking::Tag) -> Result<()> {
        each_variant!(self, repo, { repo.remove_tag(tag).await })
    }
}

#[async_trait::async_trait]
impl PayloadStorage for RepositoryHandle {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        each_variant!(self, repo, { repo.has_payload(digest).await })
    }

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        each_variant!(self, repo, { repo.iter_payload_digests() })
    }

    async unsafe fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> Result<(encoding::Digest, u64)> {
        // Safety: we are wrapping the same underlying unsafe function and
        // so the same safety holds for our callers
        unsafe { each_variant!(self, repo, { repo.write_data(reader).await }) }
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
        each_variant!(self, repo, { repo.open_payload(digest).await })
    }

    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()> {
        each_variant!(self, repo, { repo.remove_payload(digest).await })
    }
}

impl BlobStorage for RepositoryHandle {}
impl ManifestStorage for RepositoryHandle {}
impl LayerStorage for RepositoryHandle {}
impl PlatformStorage for RepositoryHandle {}

#[async_trait::async_trait]
impl DatabaseView for RepositoryHandle {
    async fn has_object(&self, digest: encoding::Digest) -> bool {
        each_variant!(self, repo, { repo.has_object(digest).await })
    }

    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        each_variant!(self, repo, { repo.read_object(digest).await })
    }

    fn find_digests(
        &self,
        search_criteria: graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        each_variant!(self, repo, { repo.find_digests(search_criteria) })
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        each_variant!(self, repo, { repo.iter_objects() })
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        each_variant!(self, repo, { repo.walk_objects(root) })
    }
}

#[async_trait::async_trait]
impl Database for RepositoryHandle {
    async fn write_object(&self, obj: &graph::Object) -> Result<()> {
        each_variant!(self, repo, { repo.write_object(obj).await })
    }

    async fn remove_object(&self, digest: encoding::Digest) -> Result<()> {
        each_variant!(self, repo, { repo.remove_object(digest).await })
    }

    async fn remove_object_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<bool> {
        each_variant!(self, repo, {
            repo.remove_object_if_older_than(older_than, digest).await
        })
    }
}

#[async_trait::async_trait]
impl Repository for Arc<RepositoryHandle> {
    /// Return the address of this repository.
    fn address(&self) -> url::Url {
        each_variant!(&**self, repo, { repo.address() })
    }

    /// Return true if this repository contains the given reference.
    async fn has_ref(&self, reference: &str) -> bool {
        each_variant!(&**self, repo, { repo.has_ref(reference).await })
    }

    /// Resolve a tag or digest string into it's absolute digest.
    async fn resolve_ref(&self, reference: &str) -> Result<encoding::Digest> {
        each_variant!(&**self, repo, { repo.resolve_ref(reference).await })
    }

    /// Read an object of unknown type by tag or digest.
    async fn read_ref(&self, reference: &str) -> Result<graph::Object> {
        each_variant!(&**self, repo, { repo.read_ref(reference).await })
    }

    /// Return the other identifiers that can be used for 'reference'.
    async fn find_aliases(&self, reference: &str) -> Result<HashSet<Ref>> {
        each_variant!(&**self, repo, { repo.find_aliases(reference).await })
    }

    /// Commit the data from 'reader' as a blob in this repository
    async fn commit_blob(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<encoding::Digest> {
        each_variant!(&**self, repo, { repo.commit_blob(reader).await })
    }
}

#[async_trait::async_trait]
impl TagStorage for Arc<RepositoryHandle> {
    async fn resolve_tag(&self, tag_spec: &tracking::TagSpec) -> Result<tracking::Tag> {
        each_variant!(&**self, repo, { repo.resolve_tag(tag_spec).await })
    }

    fn ls_tags(
        &self,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<super::EntryType>> + Send>> {
        each_variant!(&**self, repo, { repo.ls_tags(path) })
    }

    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        each_variant!(&**self, repo, { repo.find_tags(digest) })
    }

    fn iter_tag_streams(&self) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        each_variant!(&**self, repo, { repo.iter_tag_streams() })
    }

    async fn read_tag(
        &self,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        each_variant!(&**self, repo, { repo.read_tag(tag).await })
    }

    async fn insert_tag(&self, tag: &tracking::Tag) -> Result<()> {
        each_variant!(&**self, repo, { repo.insert_tag(tag).await })
    }

    async fn remove_tag_stream(&self, tag: &tracking::TagSpec) -> Result<()> {
        each_variant!(&**self, repo, { repo.remove_tag_stream(tag).await })
    }

    async fn remove_tag(&self, tag: &tracking::Tag) -> Result<()> {
        each_variant!(&**self, repo, { repo.remove_tag(tag).await })
    }
}

#[async_trait::async_trait]
impl PayloadStorage for Arc<RepositoryHandle> {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        each_variant!(&**self, repo, { repo.has_payload(digest).await })
    }

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        each_variant!(&**self, repo, { repo.iter_payload_digests() })
    }

    async unsafe fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> Result<(encoding::Digest, u64)> {
        // Safety: we are wrapping the same underlying unsafe function and
        // so the same safety holds for our callers
        unsafe { each_variant!(&**self, repo, { repo.write_data(reader).await }) }
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
        each_variant!(&**self, repo, { repo.open_payload(digest).await })
    }

    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()> {
        each_variant!(&**self, repo, { repo.remove_payload(digest).await })
    }
}

impl BlobStorage for Arc<RepositoryHandle> {}
impl ManifestStorage for Arc<RepositoryHandle> {}
impl LayerStorage for Arc<RepositoryHandle> {}
impl PlatformStorage for Arc<RepositoryHandle> {}

#[async_trait::async_trait]
impl DatabaseView for Arc<RepositoryHandle> {
    async fn has_object(&self, digest: encoding::Digest) -> bool {
        each_variant!(&**self, repo, { repo.has_object(digest).await })
    }

    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        each_variant!(&**self, repo, { repo.read_object(digest).await })
    }

    fn find_digests(
        &self,
        search_criteria: graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        each_variant!(&**self, repo, { repo.find_digests(search_criteria) })
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        each_variant!(&**self, repo, { repo.iter_objects() })
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        each_variant!(&**self, repo, { repo.walk_objects(root) })
    }
}

#[async_trait::async_trait]
impl Database for Arc<RepositoryHandle> {
    async fn write_object(&self, obj: &graph::Object) -> Result<()> {
        each_variant!(&**self, repo, { repo.write_object(obj).await })
    }

    async fn remove_object(&self, digest: encoding::Digest) -> Result<()> {
        each_variant!(&**self, repo, { repo.remove_object(digest).await })
    }

    async fn remove_object_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<bool> {
        each_variant!(&**self, repo, {
            repo.remove_object_if_older_than(older_than, digest).await
        })
    }
}
