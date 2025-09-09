// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::HashSet;
use std::pin::Pin;

//use async_stream::try_stream;
use chrono::{DateTime, Utc};
use futures::stream::select_all;
use futures::{Stream, StreamExt, future};
use relative_path::RelativePath;

use crate::config::{ToAddress, default_proxy_repo_include_secondary_tags};
use crate::graph::ObjectProto;
use crate::prelude::*;
use crate::storage::tag::TagSpecAndTagStream;
use crate::storage::{
    EntryType,
    OpenRepositoryError,
    OpenRepositoryResult,
    TagNamespace,
    TagNamespaceBuf,
    TagStorageMut,
};
use crate::tracking::BlobRead;
use crate::{Result, encoding, graph, storage, tracking};

#[cfg(test)]
#[path = "./repository_test.rs"]
mod repository_test;

/// Configuration for a proxy repository
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub primary: String,
    pub secondary: Vec<String>,
    /// Whether to include tags from secondary repos in lookup methods
    #[serde(default = "default_proxy_repo_include_secondary_tags")]
    pub include_secondary_tags: bool,
}

impl ToAddress for Config {
    fn to_address(&self) -> Result<url::Url> {
        let query = serde_qs::to_string(&self).map_err(|err| {
            crate::Error::String(format!(
                "Proxy repo parameters do not create a valid url: {err:?}"
            ))
        })?;
        url::Url::parse(&format!("proxy:?{query}")).map_err(|err| {
            crate::Error::String(format!(
                "Proxy repo config does not create a valid url: {err:?}"
            ))
        })
    }
}

#[async_trait::async_trait]
impl storage::FromUrl for Config {
    async fn from_url(url: &url::Url) -> OpenRepositoryResult<Self> {
        match url.query() {
            Some(qs) => serde_qs::from_str(qs)
                .map_err(|source| crate::storage::OpenRepositoryError::invalid_query(url, source)),
            None => Err(crate::storage::OpenRepositoryError::missing_query(url)),
        }
    }
}

/// An spfs repository that proxies for existing ones.
///
/// The proxies secondary repositories are only used to access
/// objects and tags which are missing from the primary. These
/// lookups act as a read-through and are not pulled into the
/// primary repository.
#[derive(Debug)]
pub struct ProxyRepository {
    primary: crate::storage::RepositoryHandle,
    secondary: Vec<crate::storage::RepositoryHandle>,
    include_secondary_tags: bool,
}

impl ProxyRepository {
    pub fn into_stack(self) -> Vec<crate::storage::RepositoryHandle> {
        let mut stack = vec![self.primary];
        stack.extend(self.secondary);
        stack
    }
}

#[async_trait::async_trait]
impl storage::FromConfig for ProxyRepository {
    type Config = Config;

    async fn from_config(config: Self::Config) -> OpenRepositoryResult<Self> {
        let spfs_config =
            crate::Config::current().map_err(|source| OpenRepositoryError::FailedToLoadConfig {
                source: Box::new(source),
            })?;
        #[rustfmt::skip]
        let (primary, secondary) = tokio::try_join!(
            crate::config::open_repository_from_string(&spfs_config, Some(&config.primary)),
            async {
                let mut secondary = Vec::with_capacity(config.secondary.len());
                for name in config.secondary.iter() {
                    match crate::config::open_repository_from_string(&spfs_config, Some(&name)).await? {
                        RepositoryHandle::Proxy(proxy) => {
                            // Instead of nesting proxy repos, flatten them into
                            // a single proxy repo with multiple secondaries.
                            // This helps spfs-fuse handle the case where
                            // "origin" has been changed to a proxy repo.
                            //
                            // XXX: This doesn't expand already nested proxy repos
                            secondary.extend(proxy.into_stack());
                        }
                        repo => secondary.push(repo),
                    };
                }
                Ok(secondary)
            }
        ).map_err(|source| OpenRepositoryError::FailedToOpenPartial{source: Box::new(source)})?;
        Ok(Self {
            primary,
            secondary,
            include_secondary_tags: config.include_secondary_tags,
        })
    }
}

#[async_trait::async_trait]
impl graph::DatabaseView for ProxyRepository {
    async fn has_object(&self, digest: encoding::Digest) -> bool {
        if self.primary.has_object(digest).await {
            return true;
        }

        for repo in self.secondary.iter() {
            if repo.has_object(digest).await {
                return true;
            }
        }
        false
    }

    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        let mut res = self.primary.read_object(digest).await;
        if res.is_ok() {
            return res;
        }

        for repo in self.secondary.iter() {
            if !matches!(res, Err(crate::Error::UnknownObject(_))) {
                break;
            }

            res = repo.read_object(digest).await
        }
        res
    }

    fn find_digests(
        &self,
        search_criteria: graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.primary.find_digests(search_criteria)
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        self.primary.iter_objects()
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        self.primary.walk_objects(root)
    }
}

#[async_trait::async_trait]
impl graph::Database for ProxyRepository {
    async fn remove_object(&self, digest: encoding::Digest) -> Result<()> {
        self.primary.remove_object(digest).await?;
        Ok(())
    }

    async fn remove_object_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<bool> {
        Ok(self
            .primary
            .remove_object_if_older_than(older_than, digest)
            .await?)
    }
}

#[async_trait::async_trait]
impl graph::DatabaseExt for ProxyRepository {
    async fn write_object<T: ObjectProto>(&self, obj: &graph::FlatObject<T>) -> Result<()> {
        self.primary.write_object(obj).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl PayloadStorage for ProxyRepository {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        if self.primary.has_payload(digest).await {
            return true;
        }
        for secondary in self.secondary.iter() {
            if secondary.has_payload(digest).await {
                return true;
            }
        }
        false
    }

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.primary.iter_payload_digests()
    }

    async unsafe fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> Result<(encoding::Digest, u64)> {
        // Safety: we are wrapping the same underlying unsafe function and
        // so the same safety holds for our callers
        let res = unsafe { self.primary.write_data(reader).await? };
        Ok(res)
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
        let mut res = self.primary.open_payload(digest).await;
        if res.is_ok() {
            return res;
        }

        for repo in self.secondary.iter() {
            if !matches!(res, Err(crate::Error::UnknownObject(_))) {
                break;
            }

            res = repo.open_payload(digest).await
        }
        res
    }

    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()> {
        self.primary.remove_payload(digest).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl TagStorage for ProxyRepository {
    #[inline]
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        self.primary.get_tag_namespace()
    }

    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        if self.include_secondary_tags {
            // Combine the streams of tags from all the repos taking
            // from the primary repo first.
            let primary = self.primary.ls_tags_in_namespace(namespace, path);

            let mut streams = Vec::new();
            for repo in self.secondary.iter() {
                streams.push(repo.ls_tags_in_namespace(namespace, path))
            }

            let mut seen: HashSet<String> = HashSet::new();
            Box::pin(primary.chain(select_all(streams)).filter(move |item| {
                if let Ok(entry) = item {
                    // Insert will return false if it has already been
                    // seen and this will filter out duplicates
                    future::ready(seen.insert(entry.to_string()))
                } else {
                    future::ready(true)
                }
            }))
        } else {
            // Just tags from the primary repo
            self.primary.ls_tags_in_namespace(namespace, path)
        }
    }

    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        if self.include_secondary_tags {
            // Combine the streams of tags from all the repos taking
            // from the primary repo first.
            let primary = self.primary.find_tags_in_namespace(namespace, digest);

            let mut streams = Vec::new();
            for repo in self.secondary.iter() {
                streams.push(repo.find_tags_in_namespace(namespace, digest))
            }

            let mut seen: HashSet<String> = HashSet::new();
            Box::pin(primary.chain(select_all(streams)).filter(move |item| {
                if let Ok(tag_spec) = item {
                    // Insert will return false if it has already been
                    // seen and this will filter out duplicates
                    future::ready(seen.insert(tag_spec.to_string()))
                } else {
                    future::ready(true)
                }
            }))
        } else {
            // Just tags from the primary repo
            self.primary.find_tags_in_namespace(namespace, digest)
        }
    }

    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        if self.include_secondary_tags {
            // Combine the streams of tags from all the repos taking
            // from the primary repo first.
            let primary = self.primary.iter_tag_streams_in_namespace(namespace);

            let mut streams = Vec::new();
            for repo in self.secondary.iter() {
                streams.push(repo.iter_tag_streams_in_namespace(namespace))
            }

            let mut seen: HashSet<String> = HashSet::new();
            Box::pin(primary.chain(select_all(streams)).filter(move |item| {
                if let Ok((tag_spec, _)) = item.as_ref() {
                    // Insert will return false if it has already been
                    // seen and this will filter out duplicates
                    future::ready(seen.insert(tag_spec.to_string()))
                } else {
                    future::ready(true)
                }
            }))
        } else {
            // Just tags from the primary repo
            self.primary.iter_tag_streams_in_namespace(namespace)
        }
    }

    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        let mut res = self.primary.read_tag_in_namespace(namespace, tag).await;
        if res.is_ok() {
            return res;
        }

        for repo in self.secondary.iter() {
            if !matches!(res, Err(crate::Error::UnknownReference(_))) {
                break;
            }

            res = repo.read_tag_in_namespace(namespace, tag).await
        }
        res
    }

    async fn insert_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        self.primary.insert_tag_in_namespace(namespace, tag).await?;
        Ok(())
    }

    async fn remove_tag_stream_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<()> {
        self.primary
            .remove_tag_stream_in_namespace(namespace, tag)
            .await?;
        Ok(())
    }

    async fn remove_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        self.primary.remove_tag_in_namespace(namespace, tag).await?;
        Ok(())
    }
}

impl TagStorageMut for ProxyRepository {
    fn try_set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Result<Option<TagNamespaceBuf>> {
        self.primary
            .try_as_tag_mut()
            .and_then(|tag| tag.try_set_tag_namespace(tag_namespace))
    }
}

impl Address for ProxyRepository {
    fn address(&self) -> Cow<'_, url::Url> {
        let config = Config {
            primary: self.primary.address().to_string(),
            secondary: self
                .secondary
                .iter()
                .map(|s| s.address().to_string())
                .collect(),
            include_secondary_tags: self.include_secondary_tags,
        };
        Cow::Owned(config.to_address().expect("config creates a valid url"))
    }
}
