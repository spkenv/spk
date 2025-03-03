// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::Stream;
use relative_path::RelativePath;
use spfs_encoding as encoding;

use super::prelude::*;
use super::tag::TagSpecAndTagStream;
use super::{TagNamespace, TagNamespaceBuf, TagStorageMut};
use crate::graph::ObjectProto;
use crate::tracking::{self, BlobRead};
use crate::{Error, Result, graph};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum RepositoryHandle {
    FS(super::fs::FsRepository),
    Tar(super::tar::TarRepository),
    Rpc(super::rpc::RpcRepository),
    FallbackProxy(Box<super::fallback::FallbackProxy>),
    Proxy(Box<super::proxy::ProxyRepository>),
    Pinned(Box<super::pinned::PinnedRepository<RepositoryHandle>>),
}

impl RepositoryHandle {
    /// Pin this repository to a specific date time, limiting
    /// all results to that instant and before.
    ///
    /// If this repository is already pinned, this function
    /// CAN move the pin farther into the future than it was
    /// before. In other words, pinned repositories are never
    /// nested via this function call.
    pub fn into_pinned(self, time: DateTime<Utc>) -> Self {
        match self {
            RepositoryHandle::Pinned(pinned) => Self::Pinned(Box::new(
                super::pinned::PinnedRepository::new(Arc::clone(pinned.inner()), time),
            )),
            _ => Self::Pinned(Box::new(super::pinned::PinnedRepository::new(
                Arc::new(self),
                time,
            ))),
        }
    }

    /// Make a pinned version of this repository at a specific date time,
    /// limiting all results to that instant and before.
    ///
    /// If this repository is already pinned, this function
    /// CAN move the pin farther into the future than it was
    /// before. In other words, pinned repositories are never
    /// nested via this function call.
    pub fn to_pinned(self: &Arc<Self>, time: DateTime<Utc>) -> Self {
        match &**self {
            RepositoryHandle::Pinned(pinned) => Self::Pinned(Box::new(
                super::pinned::PinnedRepository::new(Arc::clone(pinned.inner()), time),
            )),
            _ => Self::Pinned(Box::new(super::pinned::PinnedRepository::new(
                Arc::clone(self),
                time,
            ))),
        }
    }

    pub fn try_as_tag_mut(&mut self) -> Result<&mut dyn TagStorageMut> {
        match self {
            RepositoryHandle::FS(repo) => Ok(repo),
            RepositoryHandle::Tar(repo) => Ok(repo),
            RepositoryHandle::Rpc(repo) => Ok(repo),
            RepositoryHandle::FallbackProxy(repo) => Ok(&mut **repo),
            RepositoryHandle::Proxy(repo) => Ok(&mut **repo),
            RepositoryHandle::Pinned(_) => Err(Error::RepositoryIsPinned),
        }
    }
}

impl From<super::fs::FsRepository> for RepositoryHandle {
    fn from(repo: super::fs::FsRepository) -> Self {
        RepositoryHandle::FS(repo)
    }
}

impl From<super::fs::OpenFsRepository> for RepositoryHandle {
    fn from(repo: super::fs::OpenFsRepository) -> Self {
        RepositoryHandle::FS(repo.into())
    }
}

impl From<Arc<super::fs::OpenFsRepository>> for RepositoryHandle {
    fn from(repo: Arc<super::fs::OpenFsRepository>) -> Self {
        RepositoryHandle::FS(repo.into())
    }
}

impl From<super::tar::TarRepository> for RepositoryHandle {
    fn from(repo: super::tar::TarRepository) -> Self {
        RepositoryHandle::Tar(repo)
    }
}

impl From<super::rpc::RpcRepository> for RepositoryHandle {
    fn from(repo: super::rpc::RpcRepository) -> Self {
        RepositoryHandle::Rpc(repo)
    }
}

impl From<super::fallback::FallbackProxy> for RepositoryHandle {
    fn from(repo: super::fallback::FallbackProxy) -> Self {
        RepositoryHandle::FallbackProxy(Box::new(repo))
    }
}

impl From<super::proxy::ProxyRepository> for RepositoryHandle {
    fn from(repo: super::proxy::ProxyRepository) -> Self {
        RepositoryHandle::Proxy(Box::new(repo))
    }
}

impl From<Box<super::pinned::PinnedRepository<RepositoryHandle>>> for RepositoryHandle {
    fn from(repo: Box<super::pinned::PinnedRepository<RepositoryHandle>>) -> Self {
        RepositoryHandle::Pinned(repo)
    }
}

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

impl Address for RepositoryHandle {
    fn address(&self) -> Cow<'_, url::Url> {
        each_variant!(self, repo, { repo.address() })
    }
}

#[async_trait::async_trait]
impl TagStorage for RepositoryHandle {
    #[inline]
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        each_variant!(self, repo, { repo.get_tag_namespace() })
    }

    async fn resolve_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag_spec: &tracking::TagSpec,
    ) -> Result<tracking::Tag> {
        each_variant!(self, repo, {
            repo.resolve_tag_in_namespace(namespace, tag_spec).await
        })
    }

    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<super::EntryType>> + Send>> {
        each_variant!(self, repo, { repo.ls_tags_in_namespace(namespace, path) })
    }

    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        each_variant!(self, repo, {
            repo.find_tags_in_namespace(namespace, digest)
        })
    }

    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        each_variant!(self, repo, {
            repo.iter_tag_streams_in_namespace(namespace)
        })
    }

    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        each_variant!(self, repo, {
            repo.read_tag_in_namespace(namespace, tag).await
        })
    }

    async fn insert_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        each_variant!(self, repo, {
            repo.insert_tag_in_namespace(namespace, tag).await
        })
    }

    async fn remove_tag_stream_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<()> {
        each_variant!(self, repo, {
            repo.remove_tag_stream_in_namespace(namespace, tag).await
        })
    }

    async fn remove_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        each_variant!(self, repo, {
            repo.remove_tag_in_namespace(namespace, tag).await
        })
    }
}

impl TagStorageMut for RepositoryHandle {
    fn try_set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Result<Option<TagNamespaceBuf>> {
        match self {
            RepositoryHandle::FS(repo) => repo.try_set_tag_namespace(tag_namespace),
            RepositoryHandle::Tar(repo) => repo.try_set_tag_namespace(tag_namespace),
            RepositoryHandle::Rpc(repo) => repo.try_set_tag_namespace(tag_namespace),
            RepositoryHandle::FallbackProxy(repo) => repo.try_set_tag_namespace(tag_namespace),
            RepositoryHandle::Proxy(repo) => repo.try_set_tag_namespace(tag_namespace),
            RepositoryHandle::Pinned(_) => Err(Error::RepositoryIsPinned),
        }
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
impl DatabaseExt for RepositoryHandle {
    async fn write_object<T: ObjectProto>(&self, obj: &graph::FlatObject<T>) -> Result<()> {
        each_variant!(self, repo, { repo.write_object(obj).await })
    }
}

impl Address for Arc<RepositoryHandle> {
    /// Return the address of this repository.
    fn address(&self) -> Cow<'_, url::Url> {
        each_variant!(&**self, repo, { repo.address() })
    }
}

#[async_trait::async_trait]
impl TagStorage for Arc<RepositoryHandle> {
    #[inline]
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        RepositoryHandle::get_tag_namespace(self)
    }

    async fn resolve_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag_spec: &tracking::TagSpec,
    ) -> Result<tracking::Tag> {
        each_variant!(&**self, repo, {
            repo.resolve_tag_in_namespace(namespace, tag_spec).await
        })
    }

    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<super::EntryType>> + Send>> {
        each_variant!(&**self, repo, {
            repo.ls_tags_in_namespace(namespace, path)
        })
    }

    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        each_variant!(&**self, repo, {
            repo.find_tags_in_namespace(namespace, digest)
        })
    }

    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        each_variant!(&**self, repo, {
            repo.iter_tag_streams_in_namespace(namespace)
        })
    }

    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        each_variant!(&**self, repo, {
            repo.read_tag_in_namespace(namespace, tag).await
        })
    }

    async fn insert_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        each_variant!(&**self, repo, {
            repo.insert_tag_in_namespace(namespace, tag).await
        })
    }

    async fn remove_tag_stream_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<()> {
        each_variant!(&**self, repo, {
            repo.remove_tag_stream_in_namespace(namespace, tag).await
        })
    }

    async fn remove_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        each_variant!(&**self, repo, {
            repo.remove_tag_in_namespace(namespace, tag).await
        })
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

#[async_trait::async_trait]
impl DatabaseExt for Arc<RepositoryHandle> {
    async fn write_object<T: ObjectProto>(&self, obj: &graph::FlatObject<T>) -> Result<()> {
        each_variant!(&**self, repo, { repo.write_object(obj).await })
    }
}
