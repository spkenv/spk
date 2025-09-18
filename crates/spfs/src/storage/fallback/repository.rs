// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::Stream;
use relative_path::RelativePath;

use crate::config::{ToAddress, default_fallback_repo_include_secondary_tags};
use crate::graph::ObjectProto;
use crate::prelude::*;
use crate::storage::fs::{FsHashStore, ManifestRenderPath, OpenFsRepository, RenderStore};
use crate::storage::proxy::ProxyRepositoryExt;
use crate::storage::tag::TagSpecAndTagStream;
use crate::storage::{
    EntryType,
    LocalRepository,
    OpenRepositoryError,
    OpenRepositoryResult,
    TagNamespace,
    TagNamespaceBuf,
    TagStorageMut,
};
use crate::sync::reporter::SyncReporters;
use crate::tracking::BlobRead;
use crate::{Error, Result, encoding, graph, storage, tracking};

#[cfg(test)]
#[path = "./repository_test.rs"]
mod repository_test;

/// Configuration for a fallback repository
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub primary: String,
    pub secondary: Vec<String>,
    /// Whether to include tags from secondary repos in lookup methods
    #[serde(default = "default_fallback_repo_include_secondary_tags")]
    pub include_secondary_tags: bool,
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
    async fn from_url(url: &url::Url) -> crate::storage::OpenRepositoryResult<Self> {
        match url.query() {
            Some(qs) => serde_qs::from_str(qs)
                .map_err(|source| crate::storage::OpenRepositoryError::invalid_query(url, source)),
            None => Err(crate::storage::OpenRepositoryError::missing_query(url)),
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
    primary: Arc<OpenFsRepository>,
    secondary: Vec<crate::storage::RepositoryHandle>,
    include_secondary_tags: bool,
}

impl FallbackProxy {
    pub fn into_stack(self) -> Vec<crate::storage::RepositoryHandle> {
        let mut stack = vec![self.primary.into()];
        stack.extend(self.secondary);
        stack
    }

    pub fn new<P: Into<Arc<OpenFsRepository>>>(
        primary: P,
        secondary: Vec<crate::storage::RepositoryHandle>,
        include_secondary_tags: bool,
    ) -> Self {
        Self {
            primary: primary.into(),
            secondary,
            include_secondary_tags,
        }
    }
}

#[async_trait::async_trait]
impl storage::FromConfig for FallbackProxy {
    type Config = Config;

    async fn from_config(config: Self::Config) -> OpenRepositoryResult<Self> {
        let spfs_config =
            crate::Config::current().map_err(|source| OpenRepositoryError::FailedToLoadConfig {
                source: Box::new(source),
            })?;
        let primary = async {
            let primary =
                crate::config::open_repository_from_string(&spfs_config, Some(&config.primary))
                    .await
                    .map_err(|source| OpenRepositoryError::FailedToOpenPartial {
                        source: Box::new(source),
                    })?;
            let primary = match primary {
                RepositoryHandle::FS(fs) => fs,
                _ => {
                    return Err(OpenRepositoryError::UnsupportedRepositoryType(
                        "The primary repository of a FallbackProxy must be a filesystem repository"
                            .into(),
                    ));
                }
            };
            primary
                .opened()
                .await
                .map_err(|source| OpenRepositoryError::FailedToOpenPartial {
                    source: Box::new(source),
                })
        };
        let secondary = async {
            let mut secondary = Vec::with_capacity(config.secondary.len());
            for name in config.secondary.iter() {
                match crate::config::open_repository_from_string(&spfs_config, Some(&name))
                    .await
                    .map_err(|source| OpenRepositoryError::FailedToOpenPartial {
                        source: Box::new(source),
                    })? {
                    RepositoryHandle::FallbackProxy(proxy) => {
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
        };
        let (primary, secondary) = tokio::try_join!(primary, secondary)?;
        Ok(Self {
            primary: primary.into(),
            secondary,
            include_secondary_tags: config.include_secondary_tags,
        })
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
                    tracing::error!(target: "sentry", ?err, %digest, "Failed to repair missing object");

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
impl graph::DatabaseExt for FallbackProxy {
    async fn write_object<T: ObjectProto>(&self, obj: &graph::FlatObject<T>) -> Result<()> {
        self.primary.write_object(obj).await?;
        Ok(())
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
                    // Try to repair the missing blob. There can be hash
                    // collisions so use `read_blob` specifically in case
                    // there is an object of some other type with the same
                    // digest.
                    if self.read_blob(digest).await.is_ok() {
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
                        SyncReporters::silent(),
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

impl ProxyRepositoryExt for FallbackProxy {
    #[inline]
    fn include_secondary_tags(&self) -> bool {
        self.include_secondary_tags
    }

    #[inline]
    fn primary(&self) -> impl Repository {
        &*self.primary
    }

    #[inline]
    fn secondary(&self) -> &[crate::storage::RepositoryHandle] {
        &self.secondary
    }
}

#[async_trait::async_trait]
impl TagStorage for FallbackProxy {
    #[inline]
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        self.primary.get_tag_namespace()
    }

    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        crate::storage::proxy::ls_tags_in_namespace(self, namespace, path)
    }

    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        crate::storage::proxy::find_tags_in_namespace(self, namespace, digest)
    }

    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        crate::storage::proxy::iter_tag_streams_in_namespace(self, namespace)
    }

    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        crate::storage::proxy::read_tag_in_namespace(self, namespace, tag).await
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

impl TagStorageMut for FallbackProxy {
    fn try_set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Result<Option<TagNamespaceBuf>> {
        Ok(Arc::make_mut(&mut Arc::make_mut(&mut self.primary).fs_impl)
            .set_tag_namespace(tag_namespace))
    }
}

impl Address for FallbackProxy {
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
        Cow::Owned(
            config
                .to_address()
                .expect("We should not fail to create a url"),
        )
    }
}

impl LocalRepository for FallbackProxy {
    #[inline]
    fn payloads(&self) -> &FsHashStore {
        self.primary.payloads()
    }

    #[inline]
    fn render_store(&self) -> Result<&RenderStore> {
        self.primary.render_store()
    }
}

impl ManifestRenderPath for FallbackProxy {
    fn manifest_render_path(&self, manifest: &graph::Manifest) -> Result<std::path::PathBuf> {
        self.primary.manifest_render_path(manifest)
    }
}
