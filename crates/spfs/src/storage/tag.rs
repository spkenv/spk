// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::fmt::Display;
use std::pin::Pin;

use futures::Stream;
use relative_path::RelativePath;
use tokio_stream::StreamExt;

use super::{TagNamespace, TagNamespaceBuf, TAG_NAMESPACE_MARKER};
use crate::prelude::*;
use crate::{encoding, tracking, Error, Result};

pub(crate) type TagStream = Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>;
pub(crate) type TagSpecAndTagStream = (tracking::TagSpec, TagStream);
pub(crate) type IterTagsItem = Result<(tracking::TagSpec, tracking::Tag)>;

#[cfg(test)]
#[path = "./tag_test.rs"]
mod tag_test;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EntryType {
    Folder(String),
    Namespace(String),
    Tag(String),
}

impl AsRef<str> for EntryType {
    fn as_ref(&self) -> &str {
        match self {
            Self::Folder(s) => s,
            Self::Namespace(s) => s,
            Self::Tag(s) => s,
        }
    }
}

impl From<EntryType> for String {
    fn from(entry: EntryType) -> String {
        match entry {
            EntryType::Folder(s) => s,
            EntryType::Namespace(s) => s,
            EntryType::Tag(s) => s,
        }
    }
}

impl Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self {
            EntryType::Folder(e) => f.pad(format!("{e}/").as_str()),
            EntryType::Namespace(e) => f.pad(format!("{e}{TAG_NAMESPACE_MARKER}").as_str()),
            EntryType::Tag(e) => f.pad(e),
        }
    }
}

/// A location where tags are tracked and persisted.
#[async_trait::async_trait]
pub trait TagStorage: Send + Sync {
    /// Return the (optional) tag namespace to use for this tag storage.
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>>;

    /// Return true if the given tag exists in this storage.
    async fn has_tag(&self, tag: &tracking::TagSpec) -> bool {
        self.resolve_tag(tag).await.is_ok()
    }

    /// Return true if the given tag exists in this storage in the given namespace.
    async fn has_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> bool {
        self.resolve_tag_in_namespace(namespace, tag).await.is_ok()
    }

    /// Return the digest identified by the given tag spec.
    ///
    /// # Errors:
    /// - if the tag does not exist in this storage
    async fn resolve_tag(&self, tag_spec: &tracking::TagSpec) -> Result<tracking::Tag> {
        self.resolve_tag_in_namespace(self.get_tag_namespace().as_deref(), tag_spec)
            .await
    }

    /// Return the digest identified by the given tag spec in the given namespace.
    ///
    /// # Errors:
    /// - if the tag does not exist in this storage
    async fn resolve_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag_spec: &tracking::TagSpec,
    ) -> Result<tracking::Tag> {
        let mut stream =
            futures::StreamExt::enumerate(self.read_tag_in_namespace(namespace, tag_spec).await?);
        while let Some((version, tag)) = stream.next().await {
            let tag = tag?;
            if tag_spec.version() == version as u64 {
                return Ok(tag);
            }
        }
        Err(Error::UnknownReference(tag_spec.to_string()))
    }

    /// List tags and tag directories based on a tag path.
    ///
    /// For example, if the repo contains the following tags
    ///   spi/stable/my_tag
    ///   spi/stable/other_tag
    ///   spi/latest/my_tag
    /// Then ls_tags("spi") would return
    ///   stable
    ///   latest
    fn ls_tags(
        &self,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        self.ls_tags_in_namespace(self.get_tag_namespace().as_deref(), path)
    }

    /// List tags and tag directories based on a tag path and namespace.
    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>>;

    /// Find tags that point to the given digest.
    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        self.find_tags_in_namespace(self.get_tag_namespace().as_deref(), digest)
    }

    /// Find tags that point to the given digest in the given namespace.
    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>>;

    /// Iterate through the available tags in this storage.
    fn iter_tags(&self) -> Pin<Box<dyn Stream<Item = IterTagsItem> + Send>> {
        self.iter_tags_in_namespace(self.get_tag_namespace().as_deref())
    }

    /// Iterate through the available tags in this storage.
    fn iter_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = IterTagsItem> + Send>> {
        let stream = self.iter_tag_streams_in_namespace(namespace);
        let mapped = futures::StreamExt::filter_map(stream, |res| async {
            match res {
                Ok((spec, mut stream)) => match stream.next().await {
                    Some(Ok(first)) => Some(Ok((spec, first))),
                    Some(Err(err)) => Some(Err(err)),
                    None => None,
                },
                Err(err) => Some(Err(err)),
            }
        });
        Box::pin(mapped)
    }

    /// Iterate through the available tags in this storage by stream.
    fn iter_tag_streams(&self) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        self.iter_tag_streams_in_namespace(self.get_tag_namespace().as_deref())
    }

    /// Iterate through the available tags in this storage by stream in the given namespace.
    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>>;

    /// Read the entire tag stream for the given tag.
    ///
    /// If the tag does not exist, and empty stream is returned.
    async fn read_tag(
        &self,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        self.read_tag_in_namespace(self.get_tag_namespace().as_deref(), tag)
            .await
    }

    /// Read the entire tag stream for the given tag in the given namespace.
    ///
    /// If the tag does not exist, and empty stream is returned.
    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>>;

    /// Push the given tag onto the tag stream.
    async fn push_tag(
        &self,
        tag: &tracking::TagSpec,
        target: &encoding::Digest,
    ) -> Result<tracking::Tag> {
        let parent = self.resolve_tag(tag).await.ok();
        let parent_ref = match parent {
            Some(parent) => {
                // do not push redundant/unchanged head tag
                if &parent.target == target {
                    tracing::debug!("skipping tag that is already set");
                    return Ok(parent);
                }
                parent.digest()?
            }
            None => encoding::NULL_DIGEST.into(),
        };

        let mut new_tag = tracking::Tag::new(tag.org(), tag.name(), *target)?;
        new_tag.parent = parent_ref;

        self.insert_tag(&new_tag).await?;
        Ok(new_tag)
    }

    /// Insert the given tag into the tag stream, regardless of if it's valid.
    ///
    /// This insertion must sort the tag in order of datetime with any
    /// existing tags in the stream so that `read_tag` streams tags from newest
    /// to oldest.
    async fn insert_tag(&self, tag: &tracking::Tag) -> Result<()> {
        self.insert_tag_in_namespace(self.get_tag_namespace().as_deref(), tag)
            .await
    }

    /// Insert the given tag into the tag stream in the given namespace,
    /// regardless of if it's valid.
    ///
    /// This insertion must sort the tag in order of datetime with any
    /// existing tags in the stream so that `read_tag` streams tags from newest
    /// to oldest.
    async fn insert_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()>;

    /// Remove an entire tag and all related tag history.
    ///
    /// If the given tag spec contains a version, the version is ignored.
    async fn remove_tag_stream(&self, tag: &tracking::TagSpec) -> Result<()> {
        self.remove_tag_stream_in_namespace(self.get_tag_namespace().as_deref(), tag)
            .await
    }

    /// Remove an entire tag and all related tag history in the given namespace.
    ///
    /// If the given tag spec contains a version, the version is ignored.
    async fn remove_tag_stream_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<()>;

    /// Remove the oldest stored instance of the given tag.
    async fn remove_tag(&self, tag: &tracking::Tag) -> Result<()> {
        self.remove_tag_in_namespace(self.get_tag_namespace().as_deref(), tag)
            .await
    }

    /// Remove the oldest stored instance of the given tag in the given namespace.
    async fn remove_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()>;
}

#[async_trait::async_trait]
impl<T: TagStorage> TagStorage for &T {
    #[inline]
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        TagStorage::get_tag_namespace(&**self)
    }

    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        TagStorage::ls_tags_in_namespace(&**self, namespace, path)
    }

    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        TagStorage::find_tags_in_namespace(&**self, namespace, digest)
    }

    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        TagStorage::iter_tag_streams_in_namespace(&**self, namespace)
    }

    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        TagStorage::read_tag_in_namespace(&**self, namespace, tag).await
    }

    async fn insert_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        TagStorage::insert_tag_in_namespace(&**self, namespace, tag).await
    }

    async fn remove_tag_stream_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<()> {
        TagStorage::remove_tag_stream_in_namespace(&**self, namespace, tag).await
    }

    async fn remove_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        TagStorage::remove_tag_in_namespace(&**self, namespace, tag).await
    }
}

pub trait TagStorageMut {
    /// Set the configured tag namespace, returning the old tag namespace,
    /// if there was one.
    fn try_set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Result<Option<TagNamespaceBuf>>;
}
