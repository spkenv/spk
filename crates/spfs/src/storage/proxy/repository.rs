// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use chrono::{DateTime, Utc};
use futures::Stream;
use relative_path::RelativePath;

use crate::prelude::*;
use crate::storage::tag::TagSpecAndTagStream;
use crate::storage::EntryType;
use crate::tracking::BlobRead;
use crate::{encoding, graph, storage, tracking, Result};

#[cfg(test)]
#[path = "./repository_test.rs"]
mod repository_test;

/// Configuration for a proxy repository
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub primary: String,
    pub secondary: Vec<String>,
}

#[async_trait::async_trait]
impl storage::FromUrl for Config {
    async fn from_url(url: &url::Url) -> Result<Self> {
        match url.query() {
            Some(qs) => serde_qs::from_str(qs)
                .map_err(|err| crate::Error::String(format!("Invalid proxy repo url: {err:?}"))),
            None => Err(crate::Error::String(
                "Stacked repo url had empty query string, this would create an unusable repo"
                    .to_string(),
            )),
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
}

#[async_trait::async_trait]
impl storage::FromConfig for ProxyRepository {
    type Config = Config;

    async fn from_config(config: Self::Config) -> Result<Self> {
        let spfs_config = crate::Config::current()?;
        #[rustfmt::skip]
        let (primary, secondary) = tokio::try_join!(
            spfs_config.get_remote(&config.primary),
            async {
                let mut secondary = Vec::with_capacity(config.secondary.len());
                for name in config.secondary.iter() {
                    secondary.push(spfs_config.get_remote(&name).await?)
                }
                Ok(secondary)
            }
        )?;
        Ok(Self { primary, secondary })
    }
}

#[async_trait::async_trait]
impl graph::DatabaseView for ProxyRepository {
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
    async fn write_object(&self, obj: &graph::Object) -> Result<()> {
        self.primary.write_object(obj).await?;
        Ok(())
    }

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
impl PayloadStorage for ProxyRepository {
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
    fn ls_tags(
        &self,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        self.primary.ls_tags(path)
    }

    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        self.primary.find_tags(digest)
    }

    fn iter_tag_streams(&self) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        self.primary.iter_tag_streams()
    }

    async fn read_tag(
        &self,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        let mut res = self.primary.read_tag(tag).await;
        if res.is_ok() {
            return res;
        }

        for repo in self.secondary.iter() {
            if !matches!(res, Err(crate::Error::UnknownReference(_))) {
                break;
            }

            res = repo.read_tag(tag).await
        }
        res
    }

    async fn insert_tag(&self, tag: &tracking::Tag) -> Result<()> {
        self.primary.insert_tag(tag).await?;
        Ok(())
    }

    async fn remove_tag_stream(&self, tag: &tracking::TagSpec) -> Result<()> {
        self.primary.remove_tag_stream(tag).await?;
        Ok(())
    }

    async fn remove_tag(&self, tag: &tracking::Tag) -> Result<()> {
        self.primary.remove_tag(tag).await?;
        Ok(())
    }
}

impl BlobStorage for ProxyRepository {}
impl ManifestStorage for ProxyRepository {}
impl LayerStorage for ProxyRepository {}
impl PlatformStorage for ProxyRepository {}
impl Repository for ProxyRepository {
    fn address(&self) -> url::Url {
        let config = Config {
            primary: self.primary.address().to_string(),
            secondary: self
                .secondary
                .iter()
                .map(|s| s.address().to_string())
                .collect(),
        };
        let query = serde_qs::to_string(&config).expect("We should not fail to create a url");
        url::Url::parse(&format!("proxy:?{query}")).unwrap()
    }
}
