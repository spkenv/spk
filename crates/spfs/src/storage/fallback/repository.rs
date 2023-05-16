// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use chrono::{DateTime, Utc};
use futures::Stream;
use relative_path::RelativePath;

use crate::config::ToAddress;
use crate::prelude::*;
use crate::storage::fs::{FSHashStore, FSRepository, RenderStore};
use crate::storage::tag::TagSpecAndTagStream;
use crate::storage::{EntryType, LocalRepository};
use crate::tracking::BlobRead;
use crate::{encoding, graph, storage, tracking, Error, Result};

#[cfg(test)]
#[path = "./repository_test.rs"]
mod repository_test;

/// Configuration for a fallback repository
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub primary: String,
    pub secondary: Vec<String>,
}

impl ToAddress for Config {
    fn to_address(&self) -> Result<url::Url> {
        let query = serde_qs::to_string(&self).map_err(|err| {
            crate::Error::String(format!(
                "Fallback repo parameters do not create a valid url: {err:?}"
            ))
        })?;
        url::Url::parse(&format!("fallback:?{query}")).map_err(|err| {
            crate::Error::String(format!(
                "Fallback repo config does not create a valid url: {err:?}"
            ))
        })
    }
}

#[async_trait::async_trait]
impl storage::FromUrl for Config {
    async fn from_url(url: &url::Url) -> Result<Self> {
        match url.query() {
            Some(qs) => serde_qs::from_str(qs).map_err(|err| {
                crate::Error::String(format!("Invalid payload fallback repo url: {err:?}"))
            }),
            None => Err(crate::Error::String(
                "Stacked repo url had empty query string, this would create an unusable repo"
                    .to_string(),
            )),
        }
    }
}

/// An spfs repository that proxies for existing ones.
///
/// The proxy's secondary repositories are only used to repair any missing
/// payloads from the primary discovered during rendering. Any missing
/// payloads are copied into the primary repository. Missing blobs are also
/// repaired in the same way.
#[derive(Debug)]
pub struct FallbackProxy {
    // Why isn't this a RepositoryHandle?
    //
    // It needs to be something that implements LocalRepository so this
    // struct can implement it too. RepositoryHandle can't implement that
    // trait.
    primary: FSRepository,
    secondary: Vec<crate::storage::RepositoryHandle>,
}

impl FallbackProxy {
    pub fn new(primary: FSRepository, secondary: Vec<crate::storage::RepositoryHandle>) -> Self {
        Self { primary, secondary }
    }
}

#[async_trait::async_trait]
impl graph::DatabaseView for FallbackProxy {
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
            res = repo.read_object(digest).await;

            if let Ok(obj) = res.as_ref() {
                // Attempt to repair the primary repository by writing the
                // missing object. Best effort; ignore errors.
                if let Err(err) = self.primary.write_object(obj).await {
                    #[cfg(feature = "sentry")]
                    tracing::error!(target: "sentry", ?err, ?digest, "Failed to repair missing object");

                    tracing::warn!("Failed to repair missing object: {err}");
                } else {
                    #[cfg(not(feature = "sentry"))]
                    {
                        tracing::warn!("Repaired a missing object! {digest}",);
                    }
                    #[cfg(feature = "sentry")]
                    {
                        tracing::info!("Repaired a missing object! {digest}",);
                        tracing::error!(target: "sentry", object = %digest, "Repaired a missing object!");
                    }
                }
                break;
            }

            if !matches!(res, Err(crate::Error::UnknownObject(_))) {
                break;
            }
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
impl graph::Database for FallbackProxy {
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
impl PayloadStorage for FallbackProxy {
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
        let mut fallbacks = self.secondary.iter();

        'retry_open: loop {
            let missing_payload_error = match self.primary.open_payload(digest).await {
                Ok(r) => return Ok(r),
                Err(err @ Error::ObjectMissingPayload(_, _)) => err,
                Err(err @ Error::UnknownObject(_)) => {
                    // Try to repair the missing blob
                    if self.read_object(digest).await.is_ok() {
                        continue;
                    }
                    return Err(err);
                }
                Err(err) => return Err(err),
            };

            let mut repair_failure = None;

            let dest_repo = self.primary.clone().into();
            for fallback in fallbacks.by_ref() {
                let syncer = crate::Syncer::new(fallback, &dest_repo)
                    .with_policy(crate::sync::SyncPolicy::ResyncEverything)
                    .with_reporter(
                        // There may already be a progress bar in use in this
                        // context, so don't make another one here.
                        crate::sync::SilentSyncReporter::default(),
                    );
                match syncer.sync_digest(digest).await {
                    Ok(_) => {
                        // Warn for non-sentry users; info for sentry users.
                        #[cfg(not(feature = "sentry"))]
                        {
                            tracing::warn!("Repaired a missing payload! {digest}",);
                        }
                        #[cfg(feature = "sentry")]
                        {
                            tracing::info!("Repaired a missing payload! {digest}",);
                            tracing::error!(target: "sentry", object = %digest, "Repaired a missing payload!");
                        }
                        continue 'retry_open;
                    }
                    Err(err) => {
                        #[cfg(feature = "sentry")]
                        tracing::error!(
                            target: "sentry",
                            object = %digest,
                            ?err,
                            "Could not repair a missing payload"
                        );

                        repair_failure = Some(err);
                    }
                }
            }

            if let Some(err) = repair_failure {
                // The different fallbacks may fail for different reasons,
                // we just show the most recent failure here.
                tracing::warn!("Could not repair a missing payload: {err}");
            }

            return Err(missing_payload_error);
        }
    }

    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()> {
        self.primary.remove_payload(digest).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl TagStorage for FallbackProxy {
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
        self.primary.read_tag(tag).await
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

impl BlobStorage for FallbackProxy {}
impl ManifestStorage for FallbackProxy {}
impl LayerStorage for FallbackProxy {}
impl PlatformStorage for FallbackProxy {}
impl Repository for FallbackProxy {
    fn address(&self) -> url::Url {
        let config = Config {
            primary: self.primary.address().to_string(),
            secondary: self
                .secondary
                .iter()
                .map(|s| s.address().to_string())
                .collect(),
        };
        config
            .to_address()
            .expect("We should not fail to create a url")
    }
}

impl LocalRepository for FallbackProxy {
    #[inline]
    fn payloads(&self) -> &FSHashStore {
        self.primary.payloads()
    }

    #[inline]
    fn render_store(&self) -> Result<&RenderStore> {
        self.primary.render_store()
    }
}
