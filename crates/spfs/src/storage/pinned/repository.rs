// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::Stream;
use spfs_encoding as encoding;

use crate::storage::prelude::*;
use crate::tracking::BlobRead;
use crate::{graph, Error, Result};

/// PinnedRepository wraps an existing implementation,
/// limiting tag read operations to a point in time. In this setup,
/// the presence of specific objects by digest is not limited by
/// the pinned time, only the presence of, and current target for
/// tags.
///
///
/// All object write operations to pinned storage are rejected. Payloads
/// are not affected and can still be written & removed.
#[derive(PartialEq, Eq)]
pub struct PinnedRepository<T> {
    pub(super) inner: Arc<T>,
    pub pin: DateTime<Utc>,
}

impl<T> PinnedRepository<T> {
    pub fn new(inner: Arc<T>, pin: DateTime<Utc>) -> Self {
        Self { inner, pin }
    }
}

impl<T> Clone for PinnedRepository<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            pin: self.pin,
        }
    }
}

#[async_trait::async_trait]
impl<T> DatabaseView for super::PinnedRepository<T>
where
    T: DatabaseView + 'static,
{
    async fn has_object(&self, digest: encoding::Digest) -> bool {
        self.inner.has_object(digest).await
    }

    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        self.inner.read_object(digest).await
    }

    fn find_digests(
        &self,
        search_criteria: graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.inner.find_digests(search_criteria)
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        self.inner.iter_objects()
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        self.inner.walk_objects(root)
    }

    async fn resolve_full_digest(
        &self,
        partial: &encoding::PartialDigest,
    ) -> Result<encoding::Digest> {
        self.inner.resolve_full_digest(partial).await
    }
}

#[async_trait::async_trait]
impl<T> graph::Database for super::PinnedRepository<T>
where
    T: graph::Database + 'static,
{
    async fn write_object(&self, obj: &graph::Object) -> Result<()> {
        // objects are stored by digest, not time, and so can still
        // be safely written to a past repository view. In practice,
        // this allows some recovery and sync operations to still function
        // on pinned repositories
        self.inner.write_object(obj).await
    }

    async fn remove_object(&self, _digest: encoding::Digest) -> crate::Result<()> {
        Err(Error::RepositoryIsPinned)
    }

    async fn remove_object_if_older_than(
        &self,
        _older_than: DateTime<Utc>,
        _digest: encoding::Digest,
    ) -> crate::Result<bool> {
        Err(Error::RepositoryIsPinned)
    }
}

#[async_trait::async_trait]
impl<T> PayloadStorage for PinnedRepository<T>
where
    T: PayloadStorage + 'static,
{
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        self.inner.has_payload(digest).await
    }

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.inner.iter_payload_digests()
    }

    async unsafe fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> Result<(encoding::Digest, u64)> {
        // payloads are stored by digest, not time, and so can still
        // be safely written to a past repository view. In practice,
        // this allows some recovery and sync operations to still function
        // on pinned repositories

        // Safety: we are simply calling the same inner unsafe function
        unsafe { self.inner.write_data(reader).await }
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
        self.inner.open_payload(digest).await
    }

    async fn remove_payload(&self, _digest: encoding::Digest) -> Result<()> {
        Err(Error::RepositoryIsPinned)
    }
}

impl<T> BlobStorage for PinnedRepository<T> where T: BlobStorage + 'static {}
impl<T> ManifestStorage for PinnedRepository<T> where T: ManifestStorage + 'static {}
impl<T> LayerStorage for PinnedRepository<T> where T: LayerStorage + 'static {}
impl<T> PlatformStorage for PinnedRepository<T> where T: PlatformStorage + 'static {}
impl<T> Repository for PinnedRepository<T>
where
    T: Repository + 'static,
{
    fn address(&self) -> url::Url {
        let mut base = self.inner.address();
        base.query_pairs_mut()
            .append_pair("when", &self.pin.to_string());
        base
    }
}

impl<T> std::fmt::Debug for PinnedRepository<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedRepository")
            .field("pin", &self.pin)
            .field("inner", &self.inner)
            .finish()
    }
}

#[cfg(test)]
mod test {
    #[test]
    // this is a compile-time check, not a runtime test
    #[should_panic]
    fn is_repository() {
        let build =
            || -> super::PinnedRepository<crate::storage::RepositoryHandle> { unimplemented!() };
        let repo = build();
        let _: &dyn super::Repository = &repo;
    }
}
