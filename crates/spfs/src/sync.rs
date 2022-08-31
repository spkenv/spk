// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::sync::Arc;

use futures::stream::{FuturesUnordered, TryStreamExt};
use once_cell::sync::OnceCell;
use tokio::sync::{RwLock, Semaphore};

use crate::{encoding, prelude::*};
use crate::{graph, storage, tracking, Error, Result};

const DEFAULT_MAX_CONCURRENT_MANIFESTS: usize = 100;
const DEFAULT_MAX_CONCURRENT_PAYLOADS: usize = 100;

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
pub struct Syncer<'src, 'dst, Reporter: SyncReporter = SilentSyncReporter> {
    src: &'src storage::RepositoryHandle,
    dest: &'dst storage::RepositoryHandle,
    reporter: Arc<Reporter>,
    policy: SyncPolicy,
    manifest_semaphore: Arc<Semaphore>,
    payload_semaphore: Arc<Semaphore>,
    processed_digests: Arc<RwLock<HashSet<encoding::Digest>>>,
}

impl<'src, 'dst> Syncer<'src, 'dst> {
    pub fn new(
        src: &'src storage::RepositoryHandle,
        dest: &'dst storage::RepositoryHandle,
    ) -> Self {
        Self {
            src,
            dest,
            reporter: Arc::new(SilentSyncReporter::default()),
            policy: SyncPolicy::default(),
            manifest_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_MANIFESTS)),
            payload_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_PAYLOADS)),
            processed_digests: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

impl<'src, 'dst, Reporter> Syncer<'src, 'dst, Reporter>
where
    Reporter: SyncReporter,
{
    /// Creates a new syncer pulling from the provided source
    ///
    /// This new instance shares the same resource pool and cache
    /// as the one that is was cloned from. This allows them to more
    /// safely run concurrently, and to sync more efficiently
    /// by avoiding re-syncing the same objects.
    pub fn clone_with_source<'src2>(
        &self,
        source: &'src2 storage::RepositoryHandle,
    ) -> Syncer<'src2, 'dst, Reporter> {
        Syncer {
            src: source,
            dest: self.dest,
            reporter: Arc::clone(&self.reporter),
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
    pub fn with_reporter<T, R>(self, reporter: T) -> Syncer<'src, 'dst, R>
    where
        T: Into<Arc<R>>,
        R: SyncReporter,
    {
        Syncer {
            src: self.src,
            dest: self.dest,
            reporter: reporter.into(),
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
        if self.processed_digests.read().await.contains(&digest) {
            return Ok(SyncObjectResult::Duplicate);
        }
        let obj = self.read_object_with_fallback(digest).await?;
        self.sync_object(obj).await
    }

    #[async_recursion::async_recursion]
    pub async fn sync_object(&self, obj: graph::Object) -> Result<SyncObjectResult> {
        use graph::Object;
        self.reporter.visit_object(&obj);
        let res = match obj {
            Object::Layer(obj) => SyncObjectResult::Layer(self.sync_layer(obj).await?),
            Object::Platform(obj) => SyncObjectResult::Platform(self.sync_platform(obj).await?),
            Object::Blob(obj) => SyncObjectResult::Blob(self.sync_blob(obj).await?),
            Object::Manifest(obj) => SyncObjectResult::Manifest(self.sync_manifest(obj).await?),
            Object::Tree(obj) => SyncObjectResult::Tree(obj),
            Object::Mask => SyncObjectResult::Mask,
        };
        self.reporter.synced_object(&res);
        Ok(res)
    }

    pub async fn sync_platform(&self, platform: graph::Platform) -> Result<SyncPlatformResult> {
        let digest = platform.digest()?;
        if !self.processed_digests.write().await.insert(digest) {
            return Ok(SyncPlatformResult::Duplicate);
        }
        if self.policy.check_existing_objects() && self.dest.has_platform(digest).await {
            return Ok(SyncPlatformResult::Skipped);
        }
        self.reporter.visit_platform(&platform);

        let mut futures = FuturesUnordered::new();
        for digest in &platform.stack {
            futures.push(self.sync_digest(*digest));
        }
        let mut results = Vec::with_capacity(futures.len());
        while let Some(result) = futures.try_next().await? {
            results.push(result);
        }

        let platform = self.dest.create_platform(platform.stack).await?;

        let res = SyncPlatformResult::Synced { platform, results };
        self.reporter.synced_platform(&res);
        Ok(res)
    }

    pub async fn sync_layer(&self, layer: graph::Layer) -> Result<SyncLayerResult> {
        let layer_digest = layer.digest()?;
        if !self.processed_digests.write().await.insert(layer_digest) {
            return Ok(SyncLayerResult::Duplicate);
        }
        if self.policy.check_existing_objects() && self.dest.has_layer(layer_digest).await {
            return Ok(SyncLayerResult::Skipped);
        }

        self.reporter.visit_layer(&layer);
        let manifest = self.src.read_manifest(layer.manifest).await?;
        let result = self.sync_manifest(manifest).await?;
        self.dest
            .write_object(&graph::Object::Layer(layer.clone()))
            .await?;
        let res = SyncLayerResult::Synced { layer, result };
        self.reporter.synced_layer(&res);
        Ok(res)
    }

    pub async fn sync_manifest(&self, manifest: graph::Manifest) -> Result<SyncManifestResult> {
        let manifest_digest = manifest.digest()?;
        if !self.processed_digests.write().await.insert(manifest_digest) {
            return Ok(SyncManifestResult::Duplicate);
        }
        if self.policy.check_existing_objects() && self.dest.has_manifest(manifest_digest).await {
            return Ok(SyncManifestResult::Skipped);
        }
        self.reporter.visit_manifest(&manifest);
        let _permit = self.manifest_semaphore.acquire().await;
        debug_assert!(
            matches!(_permit, Ok(_)),
            "We never close the semaphore and so should never see errors"
        );

        let entries: Vec<_> = manifest
            .list_entries()
            .into_iter()
            .cloned()
            .filter(|e| e.kind.is_blob())
            .collect();
        let mut results = Vec::with_capacity(entries.len());
        let mut futures = FuturesUnordered::new();
        for entry in entries {
            futures.push(self.sync_entry(entry));
        }
        while let Some(res) = futures.try_next().await? {
            results.push(res);
        }

        self.dest
            .write_object(&graph::Object::Manifest(manifest.clone()))
            .await?;

        let res = SyncManifestResult::Synced { manifest, results };
        self.reporter.synced_manifest(&res);
        Ok(res)
    }

    async fn sync_entry(&self, entry: graph::Entry) -> Result<SyncEntryResult> {
        if !entry.kind.is_blob() {
            return Ok(SyncEntryResult::Skipped);
        }
        self.reporter.visit_entry(&entry);
        let blob = graph::Blob {
            payload: entry.object,
            size: entry.size,
        };
        let result = self.sync_blob(blob).await?;
        let res = SyncEntryResult::Synced { entry, result };
        self.reporter.synced_entry(&res);
        Ok(res)
    }

    pub async fn sync_blob(&self, blob: graph::Blob) -> Result<SyncBlobResult> {
        let digest = blob.digest();
        if self.processed_digests.read().await.contains(&digest) {
            // do not insert here because blobs share a digest with payloads
            // which should also must be visited at least once if needed
            return Ok(SyncBlobResult::Duplicate);
        }
        if self.policy.check_existing_objects() && self.dest.has_blob(digest).await {
            return Ok(SyncBlobResult::Skipped);
        }
        self.reporter.visit_blob(&blob);
        // Safety: sync_payload is unsafe to call unless the blob
        // is synced with it, which is the purpose of this function.
        let result = unsafe { self.sync_payload(blob.payload).await? };
        self.dest.write_blob(blob.clone()).await?;
        let res = SyncBlobResult::Synced { blob, result };
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
    async unsafe fn sync_payload(&self, digest: encoding::Digest) -> Result<SyncPayloadResult> {
        if !self.processed_digests.write().await.insert(digest) {
            return Ok(SyncPayloadResult::Duplicate);
        }
        if self.policy.check_existing_payloads() && self.dest.has_payload(digest).await {
            return Ok(SyncPayloadResult::Skipped);
        }

        self.reporter.visit_payload(digest);
        let _permit = self.payload_semaphore.acquire().await;
        debug_assert!(
            matches!(_permit, Ok(_)),
            "We never close the semaphore and so should never see errors"
        );
        let (payload, _) = self.src.open_payload(digest).await?;
        // Safety: this is the unsafe part where we actually create
        // the payload without a corresponsing blob
        let (created_digest, size) = unsafe { self.dest.write_data(payload).await? };
        if digest != created_digest {
            return Err(Error::String(format!(
                "Source repository provided payload that did not match the requested digest: wanted {digest}, got {created_digest}",
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

/// Receives updates from a sync process to be reported.
///
/// Unless the sync runs into errors, every call to visit_* is
/// followed up by a call to the corresponding synced_*.
pub trait SyncReporter: Send + Sync {
    /// Called when an environment has been identified to sync
    fn visit_env(&self, _env: &tracking::EnvSpec) {}

    /// Called when a environment has finished syncing
    fn synced_env(&self, _result: &SyncEnvResult) {}

    /// Called when an environment item has been identified to sync
    fn visit_env_item(&self, _item: &tracking::EnvSpecItem) {}

    /// Called when a environment item has finished syncing
    fn synced_env_item(&self, _result: &SyncEnvItemResult) {}

    /// Called when a tag has been identified to sync
    fn visit_tag(&self, _tag: &tracking::TagSpec) {}

    /// Called when a tag has finished syncing
    fn synced_tag(&self, _result: &SyncTagResult) {}

    /// Called when an object has been identified to sync
    fn visit_object(&self, _obj: &graph::Object) {}

    /// Called when a object has finished syncing
    fn synced_object(&self, _result: &SyncObjectResult) {}

    /// Called when a platform has been identified to sync
    fn visit_platform(&self, _platform: &graph::Platform) {}

    /// Called when a platform has finished syncing
    fn synced_platform(&self, _result: &SyncPlatformResult) {}

    /// Called when an environment has been identified to sync
    fn visit_layer(&self, _layer: &graph::Layer) {}

    /// Called when a layer has finished syncing
    fn synced_layer(&self, _result: &SyncLayerResult) {}

    /// Called when a manifest has been identified to sync
    fn visit_manifest(&self, _manifest: &graph::Manifest) {}

    /// Called when a manifest has finished syncing
    fn synced_manifest(&self, _result: &SyncManifestResult) {}

    /// Called when an entry has been identified to sync
    fn visit_entry(&self, _entry: &graph::Entry) {}

    /// Called when an entry has finished syncing
    fn synced_entry(&self, _result: &SyncEntryResult) {}

    /// Called when a blob has been identified to sync
    fn visit_blob(&self, _blob: &graph::Blob) {}

    /// Called when a blob has finished syncing
    fn synced_blob(&self, _result: &SyncBlobResult) {}

    /// Called when a payload has been identified to sync
    fn visit_payload(&self, _digest: encoding::Digest) {}

    /// Called when a payload has finished syncing
    fn synced_payload(&self, _result: &SyncPayloadResult) {}
}

#[derive(Default)]
pub struct SilentSyncReporter {}
impl SyncReporter for SilentSyncReporter {}

/// Reports sync progress to an interactive console via progress bars
#[derive(Default)]
pub struct ConsoleSyncReporter {
    bars: OnceCell<ConsoleSyncReporterBars>,
}

impl ConsoleSyncReporter {
    fn get_bars(&self) -> &ConsoleSyncReporterBars {
        self.bars.get_or_init(Default::default)
    }
}

impl SyncReporter for ConsoleSyncReporter {
    fn visit_manifest(&self, _manifest: &graph::Manifest) {
        self.get_bars().manifests.inc_length(1);
    }

    fn synced_manifest(&self, _result: &SyncManifestResult) {
        self.get_bars().manifests.inc(1);
    }

    fn visit_blob(&self, blob: &graph::Blob) {
        let bars = self.get_bars();
        bars.payloads.inc_length(1);
        bars.bytes.inc_length(blob.size);
    }

    fn synced_blob(&self, result: &SyncBlobResult) {
        let bars = self.get_bars();
        bars.payloads.inc(1);
        bars.bytes.inc(result.summary().synced_payload_bytes);
    }
}

struct ConsoleSyncReporterBars {
    renderer: Option<std::thread::JoinHandle<()>>,
    manifests: indicatif::ProgressBar,
    payloads: indicatif::ProgressBar,
    bytes: indicatif::ProgressBar,
}

impl Default for ConsoleSyncReporterBars {
    fn default() -> Self {
        static TICK_STRINGS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        static PROGRESS_CHARS: &str = "=>-";
        let layers_style = indicatif::ProgressStyle::default_bar()
            .template("      {spinner} {msg:<16.green} [{bar:40.cyan/dim}] {pos:>8}/{len:6}")
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let payloads_style = indicatif::ProgressStyle::default_bar()
            .template("      {spinner} {msg:<16.green} [{bar:40.cyan/dim}] {pos:>8}/{len:6}")
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let bytes_style = indicatif::ProgressStyle::default_bar()
            .template(
                "      {spinner} {msg:<16.green} [{bar:40.cyan/dim}] {bytes:>8}/{total_bytes:7}",
            )
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let bars = indicatif::MultiProgress::new();
        let manifests = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(layers_style)
                .with_message("syncing layers"),
        );
        let payloads = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(payloads_style)
                .with_message("syncing payloads"),
        );
        let bytes = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(bytes_style)
                .with_message("syncing data"),
        );
        manifests.enable_steady_tick(100);
        payloads.enable_steady_tick(100);
        bytes.enable_steady_tick(100);
        // the progress bar must be awaited from some thread
        // or nothing will be shown in the terminal
        let renderer = Some(std::thread::spawn(move || {
            if let Err(err) = bars.join() {
                tracing::error!("Failed to render sync progress: {err}");
            }
        }));
        Self {
            renderer,
            manifests,
            payloads,
            bytes,
        }
    }
}

impl Drop for ConsoleSyncReporterBars {
    fn drop(&mut self) {
        self.bytes.finish_and_clear();
        self.payloads.finish_and_clear();
        self.manifests.finish_and_clear();
        if let Some(r) = self.renderer.take() {
            let _ = r.join();
        }
    }
}

#[derive(Default, Debug)]
pub struct SyncSummary {
    /// The number of tags not synced because they already existed
    pub skipped_tags: usize,
    /// The number of tags synced
    pub synced_tags: usize,
    /// The number of objects not synced because they already existed
    pub skipped_objects: usize,
    /// The number of objects synced
    pub synced_objects: usize,
    /// The number of payloads not synced because they already existed
    pub skipped_payloads: usize,
    /// The number of payloads synced
    pub synced_payloads: usize,
    /// The total number of payload bytes synced
    pub synced_payload_bytes: u64,
}

impl SyncSummary {
    fn skipped_one_object() -> Self {
        Self {
            skipped_objects: 1,
            ..Default::default()
        }
    }

    fn synced_one_object() -> Self {
        Self {
            synced_objects: 1,
            ..Default::default()
        }
    }
}

impl std::ops::AddAssign for SyncSummary {
    fn add_assign(&mut self, rhs: Self) {
        self.skipped_tags += rhs.skipped_tags;
        self.synced_tags += rhs.synced_tags;
        self.skipped_objects += rhs.skipped_objects;
        self.synced_objects += rhs.synced_objects;
        self.skipped_payloads += rhs.skipped_payloads;
        self.synced_payloads += rhs.synced_payloads;
        self.synced_payload_bytes += rhs.synced_payload_bytes;
    }
}

impl std::iter::Sum<SyncSummary> for SyncSummary {
    fn sum<I: Iterator<Item = SyncSummary>>(iter: I) -> Self {
        iter.fold(Default::default(), |mut cur, next| {
            cur += next;
            cur
        })
    }
}

#[derive(Debug)]
pub struct SyncEnvResult {
    pub env: tracking::EnvSpec,
    pub results: Vec<SyncEnvItemResult>,
}

impl SyncEnvResult {
    pub fn summary(&self) -> SyncSummary {
        self.results.iter().map(|r| r.summary()).sum()
    }
}

#[derive(Debug)]
pub enum SyncEnvItemResult {
    Tag(SyncTagResult),
    Object(SyncObjectResult),
}

impl SyncEnvItemResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Tag(r) => r.summary(),
            Self::Object(r) => r.summary(),
        }
    }
}

#[derive(Debug)]
pub enum SyncTagResult {
    /// The tag did not need to be synced
    Skipped,
    /// The tag was already synced in this session
    Duplicate,
    /// The tag was synced
    Synced {
        tag: tracking::TagSpec,
        result: SyncObjectResult,
    },
}

impl SyncTagResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped | Self::Duplicate => SyncSummary {
                skipped_tags: 1,
                ..Default::default()
            },
            Self::Synced { result, .. } => {
                let mut summary = result.summary();
                summary.synced_tags += 1;
                summary
            }
        }
    }
}

#[derive(Debug)]
pub enum SyncObjectResult {
    /// The object was already synced in this session
    Duplicate,
    Platform(SyncPlatformResult),
    Layer(SyncLayerResult),
    Blob(SyncBlobResult),
    Manifest(SyncManifestResult),
    Tree(graph::Tree),
    Mask,
}

impl SyncObjectResult {
    pub fn summary(&self) -> SyncSummary {
        use SyncObjectResult::*;
        match self {
            Duplicate => SyncSummary {
                skipped_objects: 1,
                ..Default::default()
            },
            Platform(res) => res.summary(),
            Layer(res) => res.summary(),
            Blob(res) => res.summary(),
            Manifest(res) => res.summary(),
            Mask | Tree(_) => SyncSummary::default(),
        }
    }
}

#[derive(Debug)]
pub enum SyncPlatformResult {
    /// The platform did not need to be synced
    Skipped,
    /// The platform was already synced in this session
    Duplicate,
    /// The platform was at least partially synced
    Synced {
        platform: graph::Platform,
        results: Vec<SyncObjectResult>,
    },
}

impl SyncPlatformResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped | Self::Duplicate => SyncSummary::skipped_one_object(),
            Self::Synced { results, .. } => {
                let mut summary = results.iter().map(|r| r.summary()).sum();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

#[derive(Debug)]
pub enum SyncLayerResult {
    /// The layer did not need to be synced
    Skipped,
    /// The layer was already synced in this session
    Duplicate,
    /// The layer was synced
    Synced {
        layer: graph::Layer,
        result: SyncManifestResult,
    },
}

impl SyncLayerResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped | Self::Duplicate => SyncSummary::skipped_one_object(),
            Self::Synced { result, .. } => {
                let mut summary = result.summary();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

#[derive(Debug)]
pub enum SyncManifestResult {
    /// The manifest did not need to be synced
    Skipped,
    /// The manifest was already synced in this session
    Duplicate,
    /// The manifest was at least partially synced
    Synced {
        manifest: graph::Manifest,
        results: Vec<SyncEntryResult>,
    },
}

impl SyncManifestResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped | Self::Duplicate => SyncSummary::skipped_one_object(),
            Self::Synced { results, .. } => {
                let mut summary = results.iter().map(|r| r.summary()).sum();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

#[derive(Debug)]
pub enum SyncEntryResult {
    /// The entry did not need to be synced
    Skipped,
    /// The entry was already synced in this session
    Duplicate,
    /// The entry was synced
    Synced {
        entry: graph::Entry,
        result: SyncBlobResult,
    },
}

impl SyncEntryResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped | Self::Duplicate => SyncSummary::default(),
            Self::Synced { result, .. } => {
                let mut summary = result.summary();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

#[derive(Debug)]
pub enum SyncBlobResult {
    /// The blob did not need to be synced
    Skipped,
    /// The blob was already synced in this session
    Duplicate,
    /// The blob was synced
    Synced {
        blob: graph::Blob,
        result: SyncPayloadResult,
    },
}

impl SyncBlobResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped | Self::Duplicate => SyncSummary::skipped_one_object(),
            Self::Synced { result, .. } => {
                let mut summary = result.summary();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

#[derive(Debug)]
pub enum SyncPayloadResult {
    /// The payload did not need to be synced
    Skipped,
    /// The payload was already synced in this session
    Duplicate,
    /// The payload was synced
    Synced { size: u64 },
}

impl SyncPayloadResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped | Self::Duplicate => SyncSummary {
                skipped_payloads: 1,
                ..Default::default()
            },
            Self::Synced { size } => SyncSummary {
                synced_payloads: 1,
                synced_payload_bytes: *size,
                ..Default::default()
            },
        }
    }
}
