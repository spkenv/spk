// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use once_cell::sync::OnceCell;
use progress_bar_derive_macro::ProgressBar;

use crate::{encoding, graph, tracking};

#[derive(Clone)]
#[enum_dispatch::enum_dispatch(SyncReporter)]
pub enum SyncReporters {
    Silent(Arc<SilentSyncReporter>),
    Console(Arc<ConsoleSyncReporter>),
}

impl SyncReporters {
    /// Create a new silent reporter that does not output any progress
    pub fn silent() -> Self {
        Self::Silent(Arc::new(SilentSyncReporter::default()))
    }

    /// Create a new console reporter that shows a progress bar
    pub fn console() -> Self {
        Self::Console(Arc::new(ConsoleSyncReporter::default()))
    }
}

/// Receives updates from a sync process to be reported.
///
/// Unless the sync runs into errors, every call to visit_* is
/// followed up by a call to the corresponding synced_*.
#[enum_dispatch::enum_dispatch]
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

    /// Called when an annotation has been identified to sync
    fn visit_annotation(&self, _annotation: &graph::Annotation) {}

    /// Called when an annotation has finished syncing
    fn synced_annotation(&self, _result: &SyncAnnotationResult) {}

    /// Called when an entry has been identified to sync
    fn visit_entry(&self, _entry: &graph::Entry<'_>) {}

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

impl<T> SyncReporter for Arc<T>
where
    T: SyncReporter,
{
    fn visit_env(&self, env: &tracking::EnvSpec) {
        (**self).visit_env(env)
    }
    fn synced_env(&self, result: &SyncEnvResult) {
        (**self).synced_env(result)
    }
    fn visit_env_item(&self, item: &tracking::EnvSpecItem) {
        (**self).visit_env_item(item)
    }
    fn synced_env_item(&self, result: &SyncEnvItemResult) {
        (**self).synced_env_item(result)
    }
    fn visit_tag(&self, tag: &tracking::TagSpec) {
        (**self).visit_tag(tag)
    }
    fn synced_object(&self, result: &SyncObjectResult) {
        (**self).synced_object(result)
    }
    fn visit_platform(&self, platform: &graph::Platform) {
        (**self).visit_platform(platform)
    }
    fn synced_platform(&self, result: &SyncPlatformResult) {
        (**self).synced_platform(result)
    }
    fn visit_layer(&self, layer: &graph::Layer) {
        (**self).visit_layer(layer)
    }
    fn synced_layer(&self, result: &SyncLayerResult) {
        (**self).synced_layer(result)
    }
    fn visit_manifest(&self, manifest: &graph::Manifest) {
        (**self).visit_manifest(manifest)
    }
    fn synced_manifest(&self, result: &SyncManifestResult) {
        (**self).synced_manifest(result)
    }
    fn visit_annotation(&self, annotation: &graph::Annotation) {
        (**self).visit_annotation(annotation)
    }
    fn synced_annotation(&self, result: &SyncAnnotationResult) {
        (**self).synced_annotation(result)
    }
    fn visit_entry(&self, entry: &graph::Entry<'_>) {
        (**self).visit_entry(entry)
    }
    fn synced_entry(&self, result: &SyncEntryResult) {
        (**self).synced_entry(result)
    }
    fn visit_blob(&self, blob: &graph::Blob) {
        (**self).visit_blob(blob)
    }
    fn synced_blob(&self, result: &SyncBlobResult) {
        (**self).synced_blob(result)
    }
    fn visit_payload(&self, digest: encoding::Digest) {
        (**self).visit_payload(digest)
    }
    fn synced_payload(&self, result: &SyncPayloadResult) {
        (**self).synced_payload(result)
    }
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
        bars.bytes.inc_length(blob.size());
    }

    fn synced_blob(&self, result: &SyncBlobResult) {
        let bars = self.get_bars();
        bars.payloads.inc(1);
        bars.bytes.inc(result.summary().synced_payload_bytes);
    }

    fn synced_env(&self, _result: &SyncEnvResult) {
        let bars = self.get_bars();
        bars.manifests.abandon();
        bars.payloads.abandon();
        bars.bytes.abandon();
    }
}

#[derive(ProgressBar)]
struct ConsoleSyncReporterBars {
    #[progress_bar(
        message = "syncing layers",
        template = "      {spinner} {msg:<16.green} [{bar:40.cyan/dim}] {pos:>8}/{len:6}"
    )]
    manifests: indicatif::ProgressBar,
    #[progress_bar(
        message = "syncing payloads",
        template = "      {spinner} {msg:<16.green} [{bar:40.cyan/dim}] {pos:>8}/{len:6}"
    )]
    payloads: indicatif::ProgressBar,
    #[progress_bar(
        message = "syncing data",
        template = "      {spinner} {msg:<16.green} [{bar:40.cyan/dim}] {bytes:>8}/{total_bytes:7}"
    )]
    bytes: indicatif::ProgressBar,
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
        // destructure to ensure that all fields are processed
        // (causing compile errors for new ones that need to be added)
        let SyncSummary {
            skipped_tags,
            synced_tags,
            skipped_objects,
            synced_objects,
            skipped_payloads,
            synced_payloads,
            synced_payload_bytes,
        } = rhs;
        self.skipped_tags += skipped_tags;
        self.synced_tags += synced_tags;
        self.skipped_objects += skipped_objects;
        self.synced_objects += synced_objects;
        self.skipped_payloads += skipped_payloads;
        self.synced_payloads += synced_payloads;
        self.synced_payload_bytes += synced_payload_bytes;
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
    /// The object can be ignored, it does not need syncing, it has no
    /// representation in the database.
    Ignorable,
    Platform(SyncPlatformResult),
    Layer(SyncLayerResult),
    Blob(SyncBlobResult),
    Manifest(SyncManifestResult),
    Annotation(SyncAnnotationResult),
}

impl SyncObjectResult {
    pub fn summary(&self) -> SyncSummary {
        use SyncObjectResult as R;
        match self {
            R::Duplicate => SyncSummary {
                skipped_objects: 1,
                ..Default::default()
            },
            R::Ignorable => SyncSummary::default(),
            R::Platform(res) => res.summary(),
            R::Layer(res) => res.summary(),
            R::Blob(res) => res.summary(),
            R::Manifest(res) => res.summary(),
            R::Annotation(res) => res.summary(),
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
        results: Vec<SyncObjectResult>,
    },
}

impl SyncLayerResult {
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
pub enum SyncAnnotationResult {
    /// The annotation did not need to be synced
    InternalValue,
    /// The annotation was already synced in this session
    Duplicate,
    /// The annotation was stored in a blob and was synced
    Synced {
        digest: encoding::Digest,
        result: Box<SyncObjectResult>,
    },
}

impl SyncAnnotationResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::InternalValue | Self::Duplicate => SyncSummary::default(),
            Self::Synced {
                digest: _,
                result: _,
            } => SyncSummary::synced_one_object(),
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
    Synced { result: SyncBlobResult },
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
