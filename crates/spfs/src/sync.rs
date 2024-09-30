// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

pub mod reporter;

use std::sync::Arc;

use futures::stream::{FuturesUnordered, TryStreamExt};
use reporter::{
    SyncAnnotationResult,
    SyncBlobResult,
    SyncEntryResult,
    SyncEnvItemResult,
    SyncEnvResult,
    SyncLayerResult,
    SyncManifestResult,
    SyncObjectResult,
    SyncPayloadResult,
    SyncPlatformResult,
    SyncReporter,
    SyncReporters,
    SyncTagResult,
};
use tokio::sync::Semaphore;

use crate::graph::AnnotationValue;
use crate::prelude::*;
use crate::{encoding, graph, storage, tracking, Error, Result};

/// The default limit for concurrent manifest sync operations
/// per-syncer if not otherwise specified using
/// [`Syncer::with_max_concurrent_manifests`]
pub const DEFAULT_MAX_CONCURRENT_MANIFESTS: usize = 100;

/// The default limit for concurrent payload sync operations
/// per-syncer if not otherwise specified using
/// [`Syncer::with_max_concurrent_payloads`]
pub const DEFAULT_MAX_CONCURRENT_PAYLOADS: usize = 100;

#[cfg(test)]
#[path = "./sync_test.rs"]
mod sync_test;

/// Methods for syncing data between repositories
#[derive(Copy, Clone, Debug)]
pub enum SyncPolicy {
    /// Starting at the top-most requested item, sync and
    /// descend only into objects that are missing in the
    /// destination repo. (this is the default)
    MissingDataOnly,
    /// Update any tags to the latest target from the source
    /// repository, even if they already exist in the destination.
    /// Otherwise, follows the same semantics as [`Self::MissingDataOnly`].
    LatestTags,
    /// Same as [`Self::LatestTags`], but also descend and re-copy
    /// all object/graph data, even if it already exists in the
    /// destination. Payload data will still be skipped if it
    /// already exists.
    LatestTagsAndResyncObjects,
    /// Update to the latest target of all tags, and sync all
    /// object and payload data even if it already exists in
    /// the destination.
    ResyncEverything,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        Self::MissingDataOnly
    }
}

impl SyncPolicy {
    fn check_existing_tags(&self) -> bool {
        matches!(self, Self::MissingDataOnly)
    }

    fn check_existing_objects(&self) -> bool {
        matches!(self, Self::MissingDataOnly | Self::LatestTags)
    }

    fn check_existing_payloads(&self) -> bool {
        !matches!(self, Self::ResyncEverything)
    }
}

/// Handles the syncing of data between repositories
///
/// The syncer can be cloned efficiently
pub struct Syncer<'src, 'dst> {
    src: &'src storage::RepositoryHandle,
    dest: &'dst storage::RepositoryHandle,
    reporter: SyncReporters,
    policy: SyncPolicy,
    manifest_semaphore: Arc<Semaphore>,
    payload_semaphore: Arc<Semaphore>,
    processed_digests: Arc<dashmap::DashSet<encoding::Digest>>,
}

impl<'src, 'dst> Syncer<'src, 'dst> {
    pub fn new(
        src: &'src storage::RepositoryHandle,
        dest: &'dst storage::RepositoryHandle,
    ) -> Self {
        Self {
            src,
            dest,
            reporter: SyncReporters::silent(),
            policy: SyncPolicy::default(),
            manifest_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_MANIFESTS)),
            payload_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_PAYLOADS)),
            processed_digests: Arc::new(Default::default()),
        }
    }

    /// Creates a new syncer pulling from the provided source
    ///
    /// This new instance shares the same resource pool and cache
    /// as the one that is was cloned from. This allows them to more
    /// safely run concurrently, and to sync more efficiently
    /// by avoiding re-syncing the same objects.
    pub fn clone_with_source<'src2>(
        &self,
        source: &'src2 storage::RepositoryHandle,
    ) -> Syncer<'src2, 'dst> {
        Syncer {
            src: source,
            dest: self.dest,
            reporter: self.reporter.clone(),
            policy: self.policy,
            manifest_semaphore: Arc::clone(&self.manifest_semaphore),
            payload_semaphore: Arc::clone(&self.payload_semaphore),
            processed_digests: Arc::clone(&self.processed_digests),
        }
    }

    /// Specifies how the Syncer should deal with different types of data
    /// during the sync process, replacing any existing one.
    /// See [`SyncPolicy`].
    pub fn with_policy(mut self, policy: SyncPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Set how many manifests/layers can be processed at once.
    ///
    /// The possible total concurrent sync tasks will be the
    /// layer concurrency plus the payload concurrency.
    pub fn with_max_concurrent_manifests(mut self, concurrency: usize) -> Self {
        self.manifest_semaphore = Arc::new(Semaphore::new(concurrency));
        self
    }

    /// Set how many payloads/files can be processed at once.
    ///
    /// The possible total concurrent sync tasks will be the
    /// layer concurrency plus the payload concurrency.
    pub fn with_max_concurrent_payloads(mut self, concurrency: usize) -> Self {
        self.payload_semaphore = Arc::new(Semaphore::new(concurrency));
        self
    }

    /// Report progress to the given instance, replacing any existing one
    pub fn with_reporter(self, reporter: SyncReporters) -> Syncer<'src, 'dst> {
        Syncer {
            src: self.src,
            dest: self.dest,
            reporter,
            policy: self.policy,
            manifest_semaphore: self.manifest_semaphore,
            payload_semaphore: self.payload_semaphore,
            processed_digests: self.processed_digests,
        }
    }

    /// Sync the object(s) referenced by the given string.
    ///
    /// Any valid [`crate::tracking::EnvSpec`] is accepted as a reference.
    pub async fn sync_ref<R: AsRef<str>>(&self, reference: R) -> Result<SyncEnvResult> {
        let env_spec = reference.as_ref().parse()?;
        self.sync_env(env_spec).await
    }

    /// Sync all of the objects identified by the given env.
    pub async fn sync_env(&self, env: tracking::EnvSpec) -> Result<SyncEnvResult> {
        self.reporter.visit_env(&env);
        let mut futures = FuturesUnordered::new();
        for item in env.iter().cloned() {
            futures.push(self.sync_env_item(item));
        }
        let mut results = Vec::with_capacity(env.len());
        while let Some(result) = futures.try_next().await? {
            results.push(result);
        }
        let res = SyncEnvResult { env, results };
        self.reporter.synced_env(&res);
        Ok(res)
    }

    /// Sync one environment item and any associated data.
    pub async fn sync_env_item(&self, item: tracking::EnvSpecItem) -> Result<SyncEnvItemResult> {
        tracing::debug!(?item, "Syncing item");
        self.reporter.visit_env_item(&item);
        let res = match item {
            tracking::EnvSpecItem::Digest(digest) => self
                .sync_digest(digest)
                .await
                .map(SyncEnvItemResult::Object)?,
            tracking::EnvSpecItem::PartialDigest(digest) => self
                .sync_partial_digest(digest)
                .await
                .map(SyncEnvItemResult::Object)?,
            tracking::EnvSpecItem::TagSpec(tag_spec) => {
                self.sync_tag(tag_spec).await.map(SyncEnvItemResult::Tag)?
            }
            // These are not objects in spfs, so they are not syncable
            tracking::EnvSpecItem::LiveLayerFile(_) => {
                return Ok(SyncEnvItemResult::Object(SyncObjectResult::Ignorable))
            }
        };
        self.reporter.synced_env_item(&res);
        Ok(res)
    }

    /// Sync the identified tag instance and its target.
    pub async fn sync_tag(&self, tag: tracking::TagSpec) -> Result<SyncTagResult> {
        if self.policy.check_existing_tags() && self.dest.resolve_tag(&tag).await.is_ok() {
            return Ok(SyncTagResult::Skipped);
        }
        self.reporter.visit_tag(&tag);
        let resolved = self.src.resolve_tag(&tag).await?;
        let result = self.sync_digest(resolved.target).await?;
        self.dest.insert_tag(&resolved).await?;
        let res = SyncTagResult::Synced { tag, result };
        self.reporter.synced_tag(&res);
        Ok(res)
    }

    pub async fn sync_partial_digest(
        &self,
        partial: encoding::PartialDigest,
    ) -> Result<SyncObjectResult> {
        let mut res = self.src.resolve_full_digest(&partial).await;
        res = match res {
            Err(err) if self.policy.check_existing_objects() => {
                // there is a chance that this digest points to an existing object in
                // dest, which we don't want to fail on unless requested. In theory,
                // there is a bug here where the digest resolves to something different
                // in the destination than we expected, but being able to recover is a
                // much more useful behavior when not forcefully re-syncing as opposed
                // to avoiding this case because it allows the Syncer to be run inline
                // on environments without checking what is or is not in the destination
                // first
                self.dest
                    .resolve_full_digest(&partial)
                    .await
                    .map_err(|_| err)
            }
            res => res,
        };
        let obj = self.read_object_with_fallback(res?).await?;
        self.sync_object(obj).await
    }

    pub async fn sync_digest(&self, digest: encoding::Digest) -> Result<SyncObjectResult> {
        // don't write the digest here, as that is the responsibility
        // of the function that actually handles the data copying.
        // a short-circuit is still nice when possible, though
        if self.processed_digests.contains(&digest) {
            return Ok(SyncObjectResult::Duplicate);
        }
        let obj = self.read_object_with_fallback(digest).await?;
        self.sync_object(obj).await
    }

    #[async_recursion::async_recursion]
    pub async fn sync_object(&self, obj: graph::Object) -> Result<SyncObjectResult> {
        use graph::object::Enum;
        self.reporter.visit_object(&obj);
        let res = match obj.into_enum() {
            Enum::Layer(obj) => SyncObjectResult::Layer(self.sync_layer(obj).await?),
            Enum::Platform(obj) => SyncObjectResult::Platform(self.sync_platform(obj).await?),
            Enum::Blob(obj) => SyncObjectResult::Blob(self.sync_blob(&obj).await?),
            Enum::Manifest(obj) => SyncObjectResult::Manifest(self.sync_manifest(obj).await?),
        };
        self.reporter.synced_object(&res);
        Ok(res)
    }

    pub async fn sync_platform(&self, platform: graph::Platform) -> Result<SyncPlatformResult> {
        let digest = platform.digest()?;
        if !self.processed_digests.insert(digest) {
            return Ok(SyncPlatformResult::Duplicate);
        }
        if self.policy.check_existing_objects() && self.dest.has_object(digest).await {
            return Ok(SyncPlatformResult::Skipped);
        }
        self.reporter.visit_platform(&platform);

        let mut futures = FuturesUnordered::new();
        for digest in platform.iter_bottom_up() {
            futures.push(self.sync_digest(*digest));
        }
        let mut results = Vec::with_capacity(futures.len());
        while let Some(result) = futures.try_next().await? {
            results.push(result);
        }

        self.dest.write_object(&platform).await?;

        let res = SyncPlatformResult::Synced { platform, results };
        self.reporter.synced_platform(&res);
        Ok(res)
    }

    pub async fn sync_layer(&self, layer: graph::Layer) -> Result<SyncLayerResult> {
        let layer_digest = layer.digest()?;
        if !self.processed_digests.insert(layer_digest) {
            return Ok(SyncLayerResult::Duplicate);
        }
        if self.policy.check_existing_objects() && self.dest.has_object(layer_digest).await {
            return Ok(SyncLayerResult::Skipped);
        }

        self.reporter.visit_layer(&layer);

        let manifest_result = if let Some(manifest_digest) = layer.manifest() {
            let manifest = self.src.read_manifest(*manifest_digest).await?;
            self.sync_manifest(manifest).await?
        } else {
            SyncManifestResult::Skipped
        };
        let annotations = layer.annotations();
        let annotation_results = if annotations.is_empty() {
            vec![SyncObjectResult::Annotation(
                SyncAnnotationResult::InternalValue,
            )]
        } else {
            let mut results = Vec::with_capacity(annotations.len());
            for entry in annotations {
                results.push(SyncObjectResult::Annotation(
                    self.sync_annotation(entry.into()).await?,
                ));
            }
            results
        };

        self.dest.write_object(&layer).await?;

        let mut results = vec![SyncObjectResult::Manifest(manifest_result)];
        results.extend(annotation_results);

        let res = SyncLayerResult::Synced { layer, results };
        self.reporter.synced_layer(&res);
        Ok(res)
    }

    pub async fn sync_manifest(&self, manifest: graph::Manifest) -> Result<SyncManifestResult> {
        let manifest_digest = manifest.digest()?;
        if !self.processed_digests.insert(manifest_digest) {
            return Ok(SyncManifestResult::Duplicate);
        }
        if self.policy.check_existing_objects() && self.dest.has_object(manifest_digest).await {
            return Ok(SyncManifestResult::Skipped);
        }
        self.reporter.visit_manifest(&manifest);
        let _permit = self.manifest_semaphore.acquire().await;
        debug_assert!(
            _permit.is_ok(),
            "We never close the semaphore and so should never see errors"
        );

        let entries: Vec<_> = manifest
            .iter_entries()
            .filter(|e| e.kind().is_blob())
            .collect();
        let mut results = Vec::with_capacity(entries.len());
        let mut futures = FuturesUnordered::new();
        for entry in entries {
            futures.push(self.sync_entry(entry));
        }
        while let Some(res) = futures.try_next().await? {
            results.push(res);
        }

        self.dest.write_object(&manifest).await?;

        drop(futures);
        let res = SyncManifestResult::Synced { manifest, results };
        self.reporter.synced_manifest(&res);
        Ok(res)
    }

    async fn sync_annotation(
        &self,
        annotation: graph::Annotation<'_>,
    ) -> Result<SyncAnnotationResult> {
        match annotation.value() {
            AnnotationValue::String(_) => Ok(SyncAnnotationResult::InternalValue),
            AnnotationValue::Blob(digest) => {
                if !self.processed_digests.insert(*digest) {
                    return Ok(SyncAnnotationResult::Duplicate);
                }
                self.reporter.visit_annotation(&annotation);
                let sync_result = self.sync_digest(*digest).await?;
                let res = SyncAnnotationResult::Synced {
                    digest: *digest,
                    result: Box::new(sync_result),
                };
                self.reporter.synced_annotation(&res);
                Ok(res)
            }
        }
    }

    async fn sync_entry(&self, entry: graph::Entry<'_>) -> Result<SyncEntryResult> {
        if !entry.kind().is_blob() {
            return Ok(SyncEntryResult::Skipped);
        }
        self.reporter.visit_entry(&entry);
        let blob = graph::Blob::new(*entry.object(), entry.size());
        let result = self
            .sync_blob_with_perms_opt(&blob, Some(entry.mode()))
            .await?;
        let res = SyncEntryResult::Synced { result };
        self.reporter.synced_entry(&res);
        Ok(res)
    }

    /// Sync the identified blob to the destination repository.
    pub async fn sync_blob(&self, blob: &graph::Blob) -> Result<SyncBlobResult> {
        self.sync_blob_with_perms_opt(blob, None).await
    }

    async fn sync_blob_with_perms_opt(
        &self,
        blob: &graph::Blob,
        perms: Option<u32>,
    ) -> Result<SyncBlobResult> {
        let digest = blob.digest();
        if self.processed_digests.contains(digest) {
            // do not insert here because blobs share a digest with payloads
            // which should also must be visited at least once if needed
            return Ok(SyncBlobResult::Duplicate);
        }

        if self.policy.check_existing_objects()
            && self.dest.has_object(*digest).await
            && self.dest.has_payload(*blob.payload()).await
        {
            self.processed_digests.insert(*digest);
            return Ok(SyncBlobResult::Skipped);
        }
        self.reporter.visit_blob(blob);
        // Safety: sync_payload is unsafe to call unless the blob
        // is synced with it, which is the purpose of this function.
        let result = unsafe {
            self.sync_payload_with_perms_opt(*blob.payload(), perms)
                .await?
        };
        self.dest.write_blob(blob.to_owned()).await?;
        self.processed_digests.insert(*digest);
        let res = SyncBlobResult::Synced {
            blob: blob.to_owned(),
            result,
        };
        self.reporter.synced_blob(&res);
        Ok(res)
    }

    /// Sync a payload with the provided digest
    ///
    /// # Safety
    ///
    /// It is unsafe to call this sync function on its own,
    /// as any payload should be synced alongside its
    /// corresponding Blob instance - use [`Self::sync_blob`] instead
    pub async unsafe fn sync_payload(&self, digest: encoding::Digest) -> Result<SyncPayloadResult> {
        // Safety: these concerns are passed on to the caller
        unsafe { self.sync_payload_with_perms_opt(digest, None).await }
    }

    /// Sync a payload with the provided digest and optional set
    /// of desired permissions.
    ///
    /// # Safety
    ///
    /// It is unsafe to call this sync function on its own,
    /// as any payload should be synced alongside its
    /// corresponding Blob instance - use [`Self::sync_blob`] instead
    pub(crate) async unsafe fn sync_payload_with_perms_opt(
        &self,
        digest: encoding::Digest,
        perms: Option<u32>,
    ) -> Result<SyncPayloadResult> {
        if self.processed_digests.contains(&digest) {
            return Ok(SyncPayloadResult::Duplicate);
        }

        if self.policy.check_existing_payloads() && self.dest.has_payload(digest).await {
            return Ok(SyncPayloadResult::Skipped);
        }

        self.reporter.visit_payload(digest);
        let _permit = self.payload_semaphore.acquire().await;
        debug_assert!(
            _permit.is_ok(),
            "We never close the semaphore and so should never see errors"
        );
        let (mut payload, _) = self.src.open_payload(digest).await?;
        if let Some(perms) = perms {
            payload = Box::pin(payload.with_permissions(perms));
        }

        // Safety: this is the unsafe part where we actually create
        // the payload without a corresponding blob
        let (created_digest, size) = unsafe { self.dest.write_data(payload).await? };
        if digest != created_digest {
            return Err(Error::String(format!(
                "Source repository provided payload that did not match the requested digest: wanted {digest}, got {created_digest}. wrote {size} bytes",
            )));
        }

        let res = SyncPayloadResult::Synced { size };
        self.reporter.synced_payload(&res);
        Ok(res)
    }

    async fn read_object_with_fallback(&self, digest: encoding::Digest) -> Result<graph::Object> {
        let res = self.src.read_object(digest).await;
        match res {
            Err(err) if self.policy.check_existing_objects() => {
                // since objects are unique by digest, we can recover
                // by reading the object from our destination repository
                // on the chance that it exists
                self.dest.read_object(digest).await.map_err(|_| err)
            }
            res => res,
        }
    }
}
