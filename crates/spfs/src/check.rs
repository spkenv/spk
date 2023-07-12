// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::future::ready;
use std::sync::Arc;

use colored::Colorize;
use futures::stream::{FuturesUnordered, TryStreamExt};
use once_cell::sync::OnceCell;
use progress_bar_derive_macro::ProgressBar;
use tokio::sync::Semaphore;

use crate::prelude::*;
use crate::sync::{SyncObjectResult, SyncPayloadResult, SyncPolicy};
use crate::{encoding, graph, storage, tracking, Error, Result};

#[cfg(test)]
#[path = "./check_test.rs"]
mod check_test;

/// Handles the validation of data within a repository
///
/// The checker can be cloned efficiently
pub struct Checker<'repo, 'sync, Reporter: CheckReporter = SilentCheckReporter> {
    repo: &'repo storage::RepositoryHandle,
    repair_with: Option<super::Syncer<'sync, 'repo>>,
    reporter: Arc<Reporter>,
    processed_digests: Arc<dashmap::DashSet<encoding::Digest>>,
    tag_stream_semaphore: Semaphore,
    object_semaphore: Semaphore,
}

impl<'repo> Checker<'repo, 'static> {
    /// See [`Checker::with_max_tag_stream_concurrency`]
    pub const DEFAULT_MAX_TAG_STREAM_CONCURRENCY: usize = 1000;
    /// See [`Checker::with_max_object_concurrency`]
    pub const DEFAULT_MAX_OBJECT_CONCURRENCY: usize = 5000;

    pub fn new(repo: &'repo storage::RepositoryHandle) -> Self {
        Self {
            repo,
            reporter: Arc::new(SilentCheckReporter::default()),
            repair_with: None,
            processed_digests: Arc::new(Default::default()),
            tag_stream_semaphore: Semaphore::new(Self::DEFAULT_MAX_TAG_STREAM_CONCURRENCY),
            object_semaphore: Semaphore::new(Self::DEFAULT_MAX_OBJECT_CONCURRENCY),
        }
    }
}

impl<'repo, 'sync, Reporter> Checker<'repo, 'sync, Reporter>
where
    Reporter: CheckReporter,
{
    /// Report progress to the given instance, replacing any existing one
    pub fn with_reporter<T, R>(self, reporter: T) -> Checker<'repo, 'sync, R>
    where
        T: Into<Arc<R>>,
        R: CheckReporter,
    {
        Checker {
            repo: self.repo,
            reporter: reporter.into(),
            repair_with: self.repair_with,
            processed_digests: self.processed_digests,
            tag_stream_semaphore: self.tag_stream_semaphore,
            object_semaphore: self.object_semaphore,
        }
    }

    /// Use the provided repository to repair any missing data.
    ///
    /// When missing objects or payloads are found, the checker
    /// will attempt to sync the data seamlessly from this one,
    /// if possible.
    pub fn with_repair_source<'sync2>(
        self,
        source: &'sync2 storage::RepositoryHandle,
    ) -> Checker<'repo, 'sync2, Reporter> {
        Checker {
            repo: self.repo,
            reporter: self.reporter,
            repair_with: Some(
                crate::Syncer::new(source, self.repo)
                    .with_policy(SyncPolicy::LatestTagsAndResyncObjects),
            ),
            processed_digests: self.processed_digests,
            tag_stream_semaphore: self.tag_stream_semaphore,
            object_semaphore: self.object_semaphore,
        }
    }

    /// The maximum number of tag streams that can be read and processed at once
    pub fn with_max_tag_stream_concurrency(mut self, max_tag_stream_concurrency: usize) -> Self {
        self.tag_stream_semaphore = Semaphore::new(max_tag_stream_concurrency);
        self
    }

    /// The maximum number of objects that can be validated at once
    pub fn with_max_object_concurrency(mut self, max_object_concurrency: usize) -> Self {
        self.object_semaphore = Semaphore::new(max_object_concurrency);
        self
    }

    /// Validate that all of the targets and their children exist for all
    /// of the tags in the repository, including tag history.
    pub async fn check_all_tags(&self) -> Result<Vec<CheckTagStreamResult>> {
        self.repo
            .iter_tags()
            .and_then(|(tag, _)| ready(Ok(self.check_tag_stream(tag))))
            .try_buffer_unordered(50)
            .try_collect()
            .await
    }

    /// Validate that all of the children exist for all of the objects in the repository.
    pub async fn check_all_objects(&self) -> Result<Vec<CheckObjectResult>> {
        self.repo
            .find_digests(graph::DigestSearchCriteria::All)
            .and_then(|digest| ready(Ok(self.check_digest(digest))))
            .try_buffer_unordered(50)
            .try_collect()
            .await
    }

    /// Check the object(s) graph referenced by the given string.
    ///
    /// Any valid [`crate::tracking::EnvSpec`] is accepted as a reference.
    pub async fn check_ref<R: AsRef<str>>(&self, reference: R) -> Result<CheckEnvResult> {
        let env_spec = reference.as_ref().parse()?;
        self.check_env(env_spec).await
    }

    /// Check all of the objects identified by the given env.
    pub async fn check_env(&self, env: tracking::EnvSpec) -> Result<CheckEnvResult> {
        let results = futures::stream::iter(env.iter().cloned().map(Ok))
            .and_then(|item| ready(Ok(self.check_env_item(item))))
            .try_buffer_unordered(10)
            .try_collect()
            .await?;
        let res = CheckEnvResult { env, results };
        Ok(res)
    }

    /// Check one environment item and any associated data.
    pub async fn check_env_item(&self, item: tracking::EnvSpecItem) -> Result<CheckEnvItemResult> {
        let res = match item {
            tracking::EnvSpecItem::Digest(digest) => self
                .check_digest(digest)
                .await
                .map(CheckEnvItemResult::Object)?,
            tracking::EnvSpecItem::PartialDigest(digest) => self
                .check_partial_digest(digest)
                .await
                .map(CheckEnvItemResult::Object)?,
            tracking::EnvSpecItem::TagSpec(tag_spec) => self
                .check_tag_spec(tag_spec)
                .await
                .map(CheckEnvItemResult::Tag)?,
        };
        Ok(res)
    }

    /// Check the identified tag stream and its history.
    pub async fn check_tag_stream(&self, tag: tracking::TagSpec) -> Result<CheckTagStreamResult> {
        tracing::debug!(?tag, "Checking tag stream");
        self.reporter.visit_tag_stream(&tag);

        let _permit = self.tag_stream_semaphore.acquire().await;
        let stream = match self.repo.read_tag(&tag).await {
            Err(Error::UnknownReference(_)) => return Ok(CheckTagStreamResult::Missing),
            Err(err) => return Err(err),
            Ok(stream) => stream,
        };

        let results = stream
            .and_then(|tag| self.check_tag(tag))
            .try_collect()
            .await?;
        let res = CheckTagStreamResult::Checked { tag, results };
        self.reporter.checked_tag_stream(&res);
        Ok(res)
    }

    /// Check the identified tag and its target.
    pub async fn check_tag_spec(&self, tag: tracking::TagSpec) -> Result<CheckTagResult> {
        tracing::debug!(?tag, "Checking tag spec");
        match self.repo.resolve_tag(&tag).await {
            Err(Error::UnknownReference(_)) => Ok(CheckTagResult::Missing),
            Err(err) => Err(err),
            Ok(tag) => self.check_tag(tag).await,
        }
    }

    /// Check the identified tag instance and its target.
    pub async fn check_tag(&self, tag: tracking::Tag) -> Result<CheckTagResult> {
        tracing::debug!(?tag, "Checking tag");
        self.reporter.visit_tag(&tag);
        let result = self.check_digest(tag.target).await?;
        let res = CheckTagResult::Checked { tag, result };
        self.reporter.checked_tag(&res);
        Ok(res)
    }

    /// Validate that the identified object exists and all of its children.
    pub async fn check_partial_digest(
        &self,
        partial: encoding::PartialDigest,
    ) -> Result<CheckObjectResult> {
        let digest = self.repo.resolve_full_digest(&partial).await?;
        self.check_digest(digest).await
    }

    /// Validate that the identified object exists and all of its children.
    pub async fn check_digest(&self, digest: encoding::Digest) -> Result<CheckObjectResult> {
        self.check_digest_with_perms_opt(digest, None).await
    }

    async fn check_digest_with_perms_opt(
        &self,
        digest: encoding::Digest,
        perms: Option<u32>,
    ) -> Result<CheckObjectResult> {
        // don't write the digest here, as that is the responsibility
        // of the function that actually handles the data copying.
        // a short-circuit is still nice when possible, though
        if self.processed_digests.contains(&digest) {
            return Ok(CheckObjectResult::Duplicate);
        }

        let _permit = self.object_semaphore.acquire().await;
        tracing::trace!(?digest, "Checking digest");
        self.reporter.visit_digest(&digest);
        match self.read_object_with_fallback(digest).await {
            Err(Error::UnknownObject(_)) => Ok(CheckObjectResult::Missing(digest)),
            Err(err) => Err(err),
            Ok((obj, fallback)) => {
                let mut res = unsafe {
                    // Safety: it's unsafe to call this unless the object
                    // is known to exist, but we just loaded it from the repo
                    // or had it synced via the callback
                    self.check_object_with_perms_opt(obj, perms).await?
                };
                if matches!(fallback, Fallback::Repaired) {
                    res.set_repaired();
                }
                Ok(res)
            }
        }
    }

    /// Validate that the identified object's children all exist.
    ///
    /// To also check if the object exists, use [`Self::check_digest`]
    ///
    /// # Safety
    ///
    /// This function may sync payloads without checking blob data,
    /// which is unsafe. This function is unsafe to call unless the object
    /// is known to exist in the repository being checked
    pub async unsafe fn check_object(&self, obj: graph::Object) -> Result<CheckObjectResult> {
        // Safety: unsafe unless the object exists, we pass this up to the caller
        unsafe { self.check_object_with_perms_opt(obj, None).await }
    }

    /// Validate that all children of this object exist.
    ///
    /// Any provided permissions are associated with the blob when
    /// syncing it. See [`tracking::BlobRead::permissions`].
    ///
    /// # Safety
    ///
    /// This function may sync payloads without checking blob data,
    /// which is unsafe. This function is unsafe to call unless the object
    /// is known to exist in the repository being checked
    #[async_recursion::async_recursion]
    async unsafe fn check_object_with_perms_opt(
        &self,
        obj: graph::Object,
        perms: Option<u32>,
    ) -> Result<CheckObjectResult> {
        use graph::Object;
        if !self.processed_digests.insert(obj.digest()?) {
            return Ok(CheckObjectResult::Duplicate);
        }
        self.reporter.visit_object(&obj);
        let res = match obj {
            Object::Layer(obj) => CheckObjectResult::Layer(self.check_layer(obj).await?.into()),
            Object::Platform(obj) => CheckObjectResult::Platform(self.check_platform(obj).await?),
            Object::Blob(obj) => CheckObjectResult::Blob(unsafe {
                // Safety: it is unsafe to call this function unless the blob
                // is known to exist, which is the same rule we pass up to the caller
                self.must_check_blob_with_perms_opt(obj, perms).await?
            }),
            Object::Manifest(obj) => CheckObjectResult::Manifest(self.check_manifest(obj).await?),
            Object::Tree(obj) => CheckObjectResult::Tree(obj),
            Object::Mask => CheckObjectResult::Mask,
        };
        self.reporter.checked_object(&res);
        Ok(res)
    }

    /// Validate that the identified platform's children all exist.
    ///
    /// To also check if the platform object exists, use [`Self::check_digest`]
    pub async fn check_platform(&self, platform: graph::Platform) -> Result<CheckPlatformResult> {
        let futures: FuturesUnordered<_> = platform
            .stack
            .iter()
            .map(|d| self.check_digest(*d))
            .collect();
        let results = futures.try_collect().await?;
        let res = CheckPlatformResult {
            platform,
            results,
            repaired: false,
        };
        Ok(res)
    }

    /// Validate that the identified layer's children all exist.
    ///
    /// To also check if the layer object exists, use [`Self::check_digest`]
    pub async fn check_layer(&self, layer: graph::Layer) -> Result<CheckLayerResult> {
        let result = self.check_digest(layer.manifest).await?;
        let res = CheckLayerResult {
            layer,
            result,
            repaired: false,
        };
        Ok(res)
    }

    /// Validate that the identified manifest's children all exist.
    ///
    /// To also check if the manifest object exists, use [`Self::check_digest`]
    pub async fn check_manifest(&self, manifest: graph::Manifest) -> Result<CheckManifestResult> {
        let futures: FuturesUnordered<_> = manifest
            .iter_entries()
            .cloned()
            .filter(|e| e.kind.is_blob())
            // run through check_digest to ensure that blobs can be loaded
            // from the db and allow for possible repairs
            .map(|e| self.check_digest_with_perms_opt(e.object, Some(e.mode)))
            .collect();
        let results = futures.try_collect().await?;
        let res = CheckManifestResult {
            manifest,
            results,
            repaired: false,
        };
        Ok(res)
    }

    /// Validate that the identified blob has its payload.
    ///
    /// To also check if the blob object exists, use [`Self::check_digest`]
    ///
    /// # Safety
    /// This function may sync a payload without
    /// syncing the blob, which is unsafe unless the blob
    /// is known to exist in the repository being checked
    pub async unsafe fn check_blob(&self, blob: graph::Blob) -> Result<CheckBlobResult> {
        let digest = blob.digest();
        if !self.processed_digests.insert(digest) {
            return Ok(CheckBlobResult::Duplicate);
        }
        // Safety: this function may sync a payload and so
        // is unsafe to call unless we know the blob exists,
        // which is why this is an unsafe function
        unsafe { self.must_check_blob_with_perms_opt(blob, None).await }
    }

    /// Checks a blob, ignoring whether it has already been checked and
    /// without logging that it has been checked.
    ///
    /// Any provided permissions are associated with the blob when
    /// syncing it. See [`tracking::BlobRead::permissions`].
    ///
    /// # Safety
    ///
    /// This function may sync a payload without
    /// syncing the blob, which is unsafe unless the blob
    /// is known to exist in the repository being checked
    async unsafe fn must_check_blob_with_perms_opt(
        &self,
        blob: graph::Blob,
        perms: Option<u32>,
    ) -> Result<CheckBlobResult> {
        self.reporter.visit_blob(&blob);
        let result = unsafe {
            // Safety: this function may sync a payload and so
            // is unsafe to call unless we know the blob exists,
            // which is why this is an unsafe function
            self.check_payload_with_perms_opt(blob.payload, perms)
                .await?
        };
        let res = CheckBlobResult::Checked {
            blob,
            result,
            repaired: false,
        };
        self.reporter.checked_blob(&res);
        Ok(res)
    }

    /// Check a payload with the provided digest, repairing it if possible.
    ///
    /// # Safety
    ///
    /// This function may repair a payload, which
    /// is unsafe to do if the associated blob is not synced
    /// with it or already present.
    pub async unsafe fn check_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<CheckPayloadResult> {
        // Safety: unsafe unless the blob exists, we pass this up to the caller
        unsafe { self.check_payload_with_perms_opt(digest, None).await }
    }

    /// Any provided permissions are associated with the blob when
    /// syncing it. See [`tracking::BlobRead::permissions`].
    ///
    /// # Safety
    ///
    /// This function may repair a payload, which
    /// is unsafe to do if the associated blob is not synced
    /// with it or already present.
    async unsafe fn check_payload_with_perms_opt(
        &self,
        digest: encoding::Digest,
        perms: Option<u32>,
    ) -> Result<CheckPayloadResult> {
        self.reporter.visit_payload(digest);
        let mut result = CheckPayloadResult::Missing(digest);
        if self.repo.has_payload(digest).await {
            result = CheckPayloadResult::Ok;
        } else if let Some(syncer) = &self.repair_with {
            // Safety: this sync is unsafe unless the blob is also created
            // or exists. We pass this rule up to the caller.
            if let Ok(r) = unsafe { syncer.sync_payload_with_perms_opt(digest, perms).await } {
                self.reporter.repaired_payload(&r);
                result = CheckPayloadResult::Repaired;
            }
        }
        self.reporter.checked_payload(&result);
        Ok(result)
    }

    /// Returns the object, and whether or not it was repaired
    async fn read_object_with_fallback(
        &self,
        digest: encoding::Digest,
    ) -> Result<(graph::Object, Fallback)> {
        let res = self.repo.read_object(digest).await;
        match res {
            Err(err) => {
                let Error::UnknownObject(digest) = err else {
                    return Err(err);
                };
                let Some(syncer) = &self.repair_with else {
                    return Err(err);
                };
                if let Ok(result) = syncer.sync_digest(digest).await {
                    self.reporter.repaired_object(&result);
                    self.repo
                        .read_object(digest)
                        .await
                        .map(|o| (o, Fallback::Repaired))
                } else {
                    Err(err)
                }
            }
            res => res.map(|o| (o, Fallback::None)),
        }
    }
}

enum Fallback {
    /// No fallback was necessary, the item was already present
    None,
    /// The item was not present but was successfully repaired
    Repaired,
}

/// Receives updates from a check process to be reported.
///
/// Unless the check runs into errors, every call to visit_* is
/// followed up by a call to the corresponding checked_*.
pub trait CheckReporter: Send + Sync {
    /// Called when a tag stream has been identified to check
    fn visit_tag_stream(&self, _tag: &tracking::TagSpec) {}

    /// Called when a tag stream has finished being checked
    fn checked_tag_stream(&self, _result: &CheckTagStreamResult) {}

    /// Called when a tag has been identified to check
    fn visit_tag(&self, _tag: &tracking::Tag) {}

    /// Called when a tag has finished being checked
    fn checked_tag(&self, _result: &CheckTagResult) {}

    /// Called when an object has been identified to load and check
    fn visit_digest(&self, _digest: &encoding::Digest) {}

    /// Called when an object has been identified to check
    fn visit_object(&self, _obj: &graph::Object) {}

    /// Called when a object was found to be missing and successfully repaired
    fn repaired_object(&self, _result: &SyncObjectResult) {}

    /// Called when a object has finished being checked
    fn checked_object(&self, _result: &CheckObjectResult) {}

    /// Called when a blob has been identified to check
    fn visit_blob(&self, _blob: &graph::Blob) {}

    /// Called when a blob has finished being checked
    fn checked_blob(&self, _result: &CheckBlobResult) {}

    /// Called when a payload has been identified to check
    fn visit_payload(&self, _digest: encoding::Digest) {}

    /// Called when a payload has finished being checked
    fn checked_payload(&self, _result: &CheckPayloadResult) {}

    /// Called when a payload was found to be missing and successfully repaired
    fn repaired_payload(&self, _result: &SyncPayloadResult) {}
}

#[derive(Default)]
pub struct SilentCheckReporter {}
impl CheckReporter for SilentCheckReporter {}

/// Reports check progress to an interactive console via progress bars
#[derive(Default)]
pub struct ConsoleCheckReporter {
    bars: OnceCell<ConsoleCheckReporterBars>,
}

impl ConsoleCheckReporter {
    fn get_bars(&self) -> &ConsoleCheckReporterBars {
        self.bars.get_or_init(Default::default)
    }
}

impl CheckReporter for ConsoleCheckReporter {
    fn visit_digest(&self, _: &encoding::Digest) {
        self.get_bars().objects.inc_length(1);
    }

    fn checked_object(&self, result: &CheckObjectResult) {
        let bars = self.get_bars();
        bars.objects.inc(1);
        if let CheckObjectResult::Missing(digest) = result {
            bars.missing
                .println(format!("{}: {digest}", "Missing".red()));
            bars.missing.inc_length(1);
        }
    }

    fn repaired_object(&self, _result: &SyncObjectResult) {
        let bars = self.get_bars();
        bars.missing.inc_length(1);
        bars.missing.inc(1);
    }

    fn visit_blob(&self, blob: &graph::Blob) {
        let bars = self.get_bars();
        bars.bytes.inc_length(blob.size);
    }

    fn visit_payload(&self, _digest: encoding::Digest) {
        let bars = self.get_bars();
        bars.objects.inc_length(1);
    }

    fn checked_payload(&self, result: &CheckPayloadResult) {
        let bars = self.get_bars();
        if let CheckPayloadResult::Missing(digest) = result {
            bars.missing
                .println(format!("{}: {digest}", "Missing".red()));
            bars.missing.inc_length(1);
        }
    }

    fn repaired_payload(&self, result: &SyncPayloadResult) {
        let bars = self.get_bars();
        bars.missing.inc_length(1);
        bars.missing.inc(1);
        bars.bytes.inc(result.summary().synced_payload_bytes);
    }
}

#[derive(ProgressBar)]
struct ConsoleCheckReporterBars {
    #[progress_bar(
        message = "scanning objects",
        template = "      {spinner} {msg:<18.green} {pos:>9} reached in {elapsed:.cyan} [{per_sec}]"
    )]
    objects: indicatif::ProgressBar,
    #[progress_bar(
        message = "finding issues",
        template = "      {spinner} {msg:<18.green} {len:>9} errors, {pos} repaired ({percent}%)"
    )]
    missing: indicatif::ProgressBar,
    #[progress_bar(
        message = "payloads footprint",
        template = "      {spinner} {msg:<18.green} {total_bytes:>9} seen,   {bytes} pulled"
    )]
    bytes: indicatif::ProgressBar,
}

#[derive(Default, Debug)]
pub struct CheckSummary {
    /// The number of missing tags
    pub missing_tags: usize,
    /// The number of tags checked and found to be okay
    pub checked_tags: usize,
    /// The missing objects that were discovered
    pub missing_objects: HashSet<encoding::Digest>,
    /// The number of missing objects that were repaired
    pub repaired_objects: usize,
    /// The number of objects checked and found to be okay
    pub checked_objects: usize,
    /// The missing payloads that were discovered
    pub missing_payloads: HashSet<encoding::Digest>,
    /// The number of missing payloads that were repaired
    pub repaired_payloads: usize,
    /// The number of payloads checked and found to be okay
    pub checked_payloads: usize,
    /// The total number of payload bytes checked
    pub checked_payload_bytes: u64,
}

impl CheckSummary {
    fn checked_one_object() -> Self {
        Self {
            checked_objects: 1,
            ..Default::default()
        }
    }
}

impl std::ops::AddAssign for CheckSummary {
    fn add_assign(&mut self, rhs: Self) {
        // destructure to ensure that all fields are processed
        // (causing compile errors for new ones that need to be added)
        let CheckSummary {
            missing_tags,
            checked_tags,
            missing_objects,
            checked_objects,
            checked_payloads,
            missing_payloads,
            checked_payload_bytes,
            repaired_objects,
            repaired_payloads,
        } = rhs;
        self.missing_tags += missing_tags;
        self.checked_tags += checked_tags;
        self.missing_objects.extend(missing_objects);
        self.checked_objects += checked_objects;
        self.checked_payloads += checked_payloads;
        self.missing_payloads.extend(missing_payloads);
        self.checked_payload_bytes += checked_payload_bytes;
        self.repaired_objects += repaired_objects;
        self.repaired_payloads += repaired_payloads;
    }
}

impl std::iter::Sum<CheckSummary> for CheckSummary {
    fn sum<I: Iterator<Item = CheckSummary>>(iter: I) -> Self {
        iter.fold(Default::default(), |mut cur, next| {
            cur += next;
            cur
        })
    }
}

#[derive(Debug)]
pub struct CheckEnvResult {
    pub env: tracking::EnvSpec,
    pub results: Vec<CheckEnvItemResult>,
}

impl CheckEnvResult {
    pub fn summary(&self) -> CheckSummary {
        self.results.iter().map(|r| r.summary()).sum()
    }
}

#[derive(Debug)]
pub enum CheckEnvItemResult {
    Tag(CheckTagResult),
    Object(CheckObjectResult),
}

impl CheckEnvItemResult {
    pub fn summary(&self) -> CheckSummary {
        match self {
            Self::Tag(r) => r.summary(),
            Self::Object(r) => r.summary(),
        }
    }
}

#[derive(Debug)]
pub enum CheckTagStreamResult {
    /// The tag stream was missing from the repository
    Missing,
    /// The tag stream was checked
    Checked {
        tag: tracking::TagSpec,
        results: Vec<CheckTagResult>,
    },
}

impl CheckTagStreamResult {
    pub fn summary(&self) -> CheckSummary {
        match self {
            Self::Missing => CheckSummary {
                missing_tags: 1,
                ..Default::default()
            },
            Self::Checked { results, .. } => {
                let mut summary = CheckSummary::default();
                for result in results {
                    summary += result.summary();
                }
                summary
            }
        }
    }
}

#[derive(Debug)]
pub enum CheckTagResult {
    /// The tag was missing from the repository
    Missing,
    /// The tag was checked
    Checked {
        tag: tracking::Tag,
        result: CheckObjectResult,
    },
}

impl CheckTagResult {
    pub fn summary(&self) -> CheckSummary {
        match self {
            Self::Missing => CheckSummary {
                missing_tags: 1,
                ..Default::default()
            },
            Self::Checked { result, .. } => {
                let mut summary = result.summary();
                summary.checked_tags += 1;
                summary
            }
        }
    }
}

#[derive(Debug)]
pub enum CheckObjectResult {
    /// The object was already checked in this session
    Duplicate,
    /// The object was found to be missing from the database
    Missing(encoding::Digest),
    Platform(CheckPlatformResult),
    Layer(Box<CheckLayerResult>),
    Blob(CheckBlobResult),
    Manifest(CheckManifestResult),
    Tree(graph::Tree),
    Mask,
}

impl CheckObjectResult {
    /// Marks this result as being repaired, if possible.
    /// A [`Self::Missing`] or [`Self::Duplicate`] result cannot be changed this way.
    fn set_repaired(&mut self) {
        match self {
            CheckObjectResult::Duplicate => (),
            CheckObjectResult::Missing(_) => (),
            CheckObjectResult::Platform(r) => r.set_repaired(),
            CheckObjectResult::Layer(r) => r.set_repaired(),
            CheckObjectResult::Blob(r) => r.set_repaired(),
            CheckObjectResult::Manifest(r) => r.set_repaired(),
            CheckObjectResult::Tree(_) => (),
            CheckObjectResult::Mask => (),
        }
    }

    pub fn summary(&self) -> CheckSummary {
        use CheckObjectResult::*;
        match self {
            Duplicate => CheckSummary::default(),
            Missing(digest) => CheckSummary {
                missing_objects: Some(*digest).into_iter().collect(),
                ..Default::default()
            },
            Platform(res) => res.summary(),
            Layer(res) => res.summary(),
            Blob(res) => res.summary(),
            Manifest(res) => res.summary(),
            Mask | Tree(_) => CheckSummary::default(),
        }
    }
}

#[derive(Debug)]
pub struct CheckPlatformResult {
    pub repaired: bool,
    pub platform: graph::Platform,
    pub results: Vec<CheckObjectResult>,
}

impl CheckPlatformResult {
    /// Marks this result as being repaired.
    fn set_repaired(&mut self) {
        self.repaired = true;
    }

    pub fn summary(&self) -> CheckSummary {
        let mut summary: CheckSummary = self.results.iter().map(|r| r.summary()).sum();
        summary += CheckSummary::checked_one_object();
        if self.repaired {
            summary.repaired_objects += 1;
        }
        summary
    }
}

#[derive(Debug)]
pub struct CheckLayerResult {
    pub repaired: bool,
    pub layer: graph::Layer,
    pub result: CheckObjectResult,
}

impl CheckLayerResult {
    /// Marks this result as being repaired.
    fn set_repaired(&mut self) {
        self.repaired = true;
    }

    pub fn summary(&self) -> CheckSummary {
        let mut summary = self.result.summary();
        summary += CheckSummary::checked_one_object();
        if self.repaired {
            summary.repaired_objects += 1;
        }
        summary
    }
}

#[derive(Debug)]
pub struct CheckManifestResult {
    pub repaired: bool,
    pub manifest: graph::Manifest,
    pub results: Vec<CheckObjectResult>,
}

impl CheckManifestResult {
    /// Marks this result as being repaired.
    fn set_repaired(&mut self) {
        self.repaired = true;
    }

    pub fn summary(&self) -> CheckSummary {
        let mut summary: CheckSummary = self.results.iter().map(|r| r.summary()).sum();
        summary += CheckSummary::checked_one_object();
        if self.repaired {
            summary.repaired_objects += 1;
        }
        summary
    }
}

#[derive(Debug)]
pub enum CheckEntryResult {
    /// The entry was not one that needed checking
    Skipped,
    /// The entry was checked
    Checked {
        entry: graph::Entry,
        result: CheckBlobResult,
    },
}

impl CheckEntryResult {
    pub fn summary(&self) -> CheckSummary {
        match self {
            Self::Skipped => CheckSummary::default(),
            Self::Checked { result, .. } => result.summary(),
        }
    }
}

#[derive(Debug)]
pub enum CheckBlobResult {
    /// The blob was already checked in this session
    Duplicate,
    /// The blob was not found in the database
    Missing(encoding::Digest),
    /// The blob was checked
    Checked {
        repaired: bool,
        blob: graph::Blob,
        result: CheckPayloadResult,
    },
}

impl CheckBlobResult {
    /// Marks this result as being repaired.
    fn set_repaired(&mut self) {
        if let Self::Checked { repaired, .. } = self {
            *repaired = true;
        }
    }

    pub fn summary(&self) -> CheckSummary {
        match self {
            Self::Duplicate => CheckSummary::default(),
            Self::Missing(digest) => CheckSummary {
                missing_objects: Some(*digest).into_iter().collect(),
                ..Default::default()
            },
            Self::Checked {
                repaired,
                result,
                blob,
            } => {
                let mut summary = result.summary();
                summary += CheckSummary {
                    checked_objects: 1,
                    checked_payload_bytes: blob.size,
                    repaired_objects: *repaired as usize,
                    ..Default::default()
                };
                summary
            }
        }
    }
}

#[derive(Debug)]
pub enum CheckPayloadResult {
    /// The payload is missing from this repository
    Missing(encoding::Digest),
    /// The payload was missing from this repository but was repaired
    Repaired,
    /// The payload was checked and is present
    Ok,
}

impl CheckPayloadResult {
    pub fn summary(&self) -> CheckSummary {
        match self {
            Self::Missing(digest) => CheckSummary {
                missing_payloads: Some(*digest).into_iter().collect(),
                ..Default::default()
            },
            Self::Repaired => CheckSummary {
                checked_payloads: 1,
                repaired_payloads: 1,
                ..Default::default()
            },
            Self::Ok => CheckSummary {
                checked_payloads: 1,
                ..Default::default()
            },
        }
    }
}
