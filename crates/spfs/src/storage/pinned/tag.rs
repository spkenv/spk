// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::future::ready;
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt, TryStreamExt};
use relative_path::RelativePath;

use super::PinnedRepository;
use crate::storage::tag::{EntryType, TagSpecAndTagStream, TagStream};
use crate::storage::TagStorage;
use crate::{encoding, tracking, Error, Result};

#[cfg(test)]
#[path = "./tag_test.rs"]
mod tag_test;

#[async_trait::async_trait]
impl<T> TagStorage for PinnedRepository<T>
where
    T: TagStorage + 'static,
{
    /// Return true if the given tag exists in this storage.
    async fn has_tag(&self, tag: &tracking::TagSpec) -> bool {
        self.read_tag(tag).await.is_ok()
    }

    /// Return the digest identified by the given tag spec.
    ///
    /// # Errors:
    /// - if the tag does not exist in this storage
    async fn resolve_tag(&self, tag_spec: &tracking::TagSpec) -> Result<tracking::Tag> {
        let mut stream = futures::StreamExt::enumerate(self.read_tag(tag_spec).await?);
        while let Some((version, tag)) = stream.next().await {
            let tag = tag?;
            if tag_spec.version() == version as u64 {
                return Ok(tag);
            }
        }
        Err(Error::UnknownReference(tag_spec.to_string()))
    }

    fn ls_tags(
        &self,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        let path = path.to_owned();
        let repo = self.clone();
        let source = repo.inner.ls_tags(&path);
        Box::pin(source.try_filter_map(move |entry| {
            let repo = repo.clone();
            let entry_path = path.join(entry.to_string());
            async move {
                Ok(match &entry {
                    EntryType::Folder(_) => repo.has_tag_folder(&entry_path).await.then_some(entry),
                    EntryType::Tag(_) => {
                        let spec = tracking::TagSpec::parse(entry_path).unwrap();
                        repo.has_tag(&spec).await.then_some(entry)
                    }
                })
            }
        }))
    }

    /// Find tags that point to the given digest.
    ///
    /// This is an O(n) operation based on the number of all
    /// tag versions in each tag stream.
    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        let inner = Arc::clone(&self.inner);
        let source = inner.find_tags(digest);
        Box::pin(source.try_filter(move |t| {
            let t = t.clone();
            let inner = Arc::clone(&inner);
            async move { inner.has_tag(&t).await }
        }))
    }

    /// Iterate through the available tags in this storage.
    fn iter_tags(&self) -> Pin<Box<dyn Stream<Item = crate::storage::tag::IterTagsItem> + Send>> {
        let stream = self.iter_tag_streams();
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

    /// Iterate through the available tags in this storage.
    fn iter_tag_streams(&self) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        let inner = Arc::clone(&self.inner);
        let source = inner.iter_tag_streams();
        let pin = self.pin;
        Box::pin(source.try_filter_map(move |(tag, stream)| async move {
            let mut peekable = stream.peekable();
            while let Some(next_tag) = Pin::new(&mut peekable).peek().await {
                // return streams that have an entry less than the pin time
                // or in cases when we run into an error. Although the error
                // case is potentially identifying a tag that shouldn't exist
                // in this pin view, the alternative is to silently ignore the
                // error which seems at least equally undesirable
                if next_tag
                    .as_ref()
                    .ok()
                    .map(|t| t.time <= pin)
                    .unwrap_or(true)
                {
                    let filtered: TagStream = Box::pin(peekable);
                    return Ok(Some((tag, filtered)));
                }
                let _ = peekable.next().await;
            }
            Ok(None)
        }))
    }

    async fn read_tag(
        &self,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        let pin = self.pin;
        let inner = Arc::clone(&self.inner);
        let mut source = inner
            .read_tag(tag)
            .await?
            .try_filter(move |tag| ready(tag.time <= pin))
            .peekable();
        if Pin::new(&mut source).peek().await.is_none() {
            return Err(Error::UnknownReference(tag.to_string()));
        }
        Ok(Box::pin(source))
    }

    /// Push the given tag onto the tag stream.
    async fn push_tag(
        &self,
        _tag: &tracking::TagSpec,
        _target: &encoding::Digest,
    ) -> Result<tracking::Tag> {
        Err(Error::RepositoryIsPinned)
    }

    async fn insert_tag(&self, _tag: &tracking::Tag) -> Result<()> {
        Err(Error::RepositoryIsPinned)
    }

    async fn remove_tag_stream(&self, _tag: &tracking::TagSpec) -> Result<()> {
        Err(Error::RepositoryIsPinned)
    }

    async fn remove_tag(&self, _tag: &tracking::Tag) -> Result<()> {
        Err(Error::RepositoryIsPinned)
    }
}

impl<T> PinnedRepository<T>
where
    T: TagStorage + 'static,
{
    /// True if the provided tag folder has any entries in this view
    ///
    /// This operation needs to find a tag under the provided root with at least
    /// one entry before the pin time and so the operation is O(n) where n is
    /// the total number of tag versions in the hierarchy.
    async fn has_tag_folder(&self, path: &relative_path::RelativePath) -> bool {
        self.ls_tags(path).any(|r| ready(r.is_ok())).await
    }
}
