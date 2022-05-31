// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use tokio_stream::StreamExt;

use crate::{encoding, prelude::*};
use crate::{graph, storage, tracking, Error, Result};

#[cfg(test)]
#[path = "./sync_test.rs"]
mod sync_test;

/// Limits the concurrency in sync operations to avoid
/// connection and open file descriptor limits
// TODO: load this from the config
static MAX_CONCURRENT: usize = 256;

/// Handles the syncing of data between repositories
pub struct Syncer<'src, 'dst> {
    src: &'src storage::RepositoryHandle,
    dest: &'dst storage::RepositoryHandle,
    skip_existing_tags: bool,
    skip_existing_objects: bool,
    skip_existing_payloads: bool,
}

impl<'src, 'dst> Syncer<'src, 'dst> {
    pub fn new(
        src: &'src storage::RepositoryHandle,
        dest: &'dst storage::RepositoryHandle,
    ) -> Self {
        Self {
            src,
            dest,
            skip_existing_tags: true,
            skip_existing_objects: true,
            skip_existing_payloads: true,
        }
    }

    /// When true, do not sync any tag that already exists in the destination repo.
    ///
    /// This is on by default, but can be disabled in order to retrieve updated tag
    /// information from the source repo.
    pub fn set_skip_existing_tags(&mut self, skip_existing: bool) -> &mut Self {
        self.skip_existing_tags = skip_existing;
        if skip_existing {
            self.skip_existing_payloads = true;
        }
        self
    }

    /// When true, do not sync any object that already exists in the destination repo.
    ///
    /// This is on by default, but can be disabled in order to repair a corrupt
    /// repository that has some parent object but is missing one or more children.
    ///
    /// Setting this to true will also skip any existing payload.
    pub fn set_skip_existing_objects(&mut self, skip_existing: bool) -> &mut Self {
        self.skip_existing_objects = skip_existing;
        if skip_existing {
            self.skip_existing_payloads = true;
        }
        self
    }

    /// When true, do not sync any payload that already exists in the destination repo.
    ///
    /// This is on by default, but can be disabled in order to repair a corrupt
    /// repository. Setting this to true, also means that existing objects will not
    /// be skipped.
    pub fn set_skip_existing_payloads(&mut self, skip_existing: bool) -> &mut Self {
        self.skip_existing_payloads = skip_existing;
        if !skip_existing {
            self.skip_existing_objects = false;
        }
        self
    }

    /// Sync the object(s) referenced by the given string.
    ///
    /// Any valid [`spfs::tracking::EnvSpec`] is accepted as a reference.
    pub async fn sync_ref<R: AsRef<str>>(&self, reference: R) -> Result<SyncEnvResult> {
        let env_spec = reference.as_ref().parse()?;
        self.sync_env(env_spec).await
    }

    /// Sync all of the objects identified by the given env.
    pub async fn sync_env(&self, env: tracking::EnvSpec) -> Result<SyncEnvResult> {
        let mut results = Vec::with_capacity(env.len());
        for item in env.iter().cloned() {
            results.push(self.sync_env_item(item).await?);
        }
        Ok(SyncEnvResult { env, results })
    }

    /// Sync one environment item and any associated data.
    pub async fn sync_env_item(&self, item: tracking::EnvSpecItem) -> Result<SyncEnvItemResult> {
        match item {
            tracking::EnvSpecItem::Digest(digest) => self
                .sync_digest(digest)
                .await
                .map(SyncEnvItemResult::Object),
            tracking::EnvSpecItem::PartialDigest(digest) => self
                .sync_partial_digest(digest)
                .await
                .map(SyncEnvItemResult::Object),
            tracking::EnvSpecItem::TagSpec(tag_spec) => {
                self.sync_tag(tag_spec).await.map(SyncEnvItemResult::Tag)
            }
        }
    }

    /// Sync the identified tag instance and its target.
    pub async fn sync_tag(&self, tag: tracking::TagSpec) -> Result<SyncTagResult> {
        if self.skip_existing_tags && self.dest.resolve_tag(&tag).await.is_ok() {
            return Ok(SyncTagResult::Skipped);
        }
        let resolved = self.src.resolve_tag(&tag).await?;
        let result = self.sync_digest(resolved.target).await?;
        tracing::debug!(tag = ?tag.path(), "syncing tag");
        self.dest.push_raw_tag(&resolved).await?;
        Ok(SyncTagResult::Synced { tag, result })
    }

    pub async fn sync_partial_digest(
        &self,
        partial: encoding::PartialDigest,
    ) -> Result<SyncObjectResult> {
        let digest = self.src.resolve_full_digest(&partial).await?;
        let obj = self.src.read_object(digest).await?;
        self.sync_object(obj).await
    }

    pub async fn sync_digest(&self, digest: encoding::Digest) -> Result<SyncObjectResult> {
        let obj = self.src.read_object(digest).await?;
        self.sync_object(obj).await
    }

    #[async_recursion::async_recursion]
    pub async fn sync_object(&self, obj: graph::Object) -> Result<SyncObjectResult> {
        use graph::Object;
        match obj {
            Object::Layer(obj) => Ok(SyncObjectResult::Layer(self.sync_layer(obj).await?)),
            Object::Platform(obj) => Ok(SyncObjectResult::Platform(self.sync_platform(obj).await?)),
            Object::Blob(obj) => Ok(SyncObjectResult::Blob(self.sync_blob(obj).await?)),
            Object::Manifest(obj) => Ok(SyncObjectResult::Manifest(self.sync_manifest(obj).await?)),
            Object::Tree(obj) => Ok(SyncObjectResult::Tree(obj)),
            Object::Mask => Ok(SyncObjectResult::Mask),
        }
    }

    pub async fn sync_platform(&self, platform: graph::Platform) -> Result<SyncPlatformResult> {
        let digest = platform.digest()?;
        if self.skip_existing_objects && self.dest.has_platform(digest).await {
            tracing::debug!(?digest, "platform already synced");
            return Ok(SyncPlatformResult::Skipped);
        }
        tracing::info!(?digest, "syncing platform");
        let mut results = Vec::with_capacity(platform.stack.len());
        for digest in &platform.stack {
            let obj = self.src.read_object(*digest).await?;
            results.push(self.sync_object(obj).await?);
        }

        let platform = self.dest.create_platform(platform.stack).await?;

        Ok(SyncPlatformResult::Synced { platform, results })
    }

    pub async fn sync_layer(&self, layer: graph::Layer) -> Result<SyncLayerResult> {
        let layer_digest = layer.digest()?;
        if self.skip_existing_objects && self.dest.has_layer(layer_digest).await {
            tracing::debug!(digest = ?layer_digest, "layer already synced");
            return Ok(SyncLayerResult::Skipped);
        }

        tracing::info!(digest = ?layer_digest, "syncing layer");
        let manifest = self.src.read_manifest(layer.manifest).await?;
        let result = self.sync_manifest(manifest).await?;
        self.dest
            .write_object(&graph::Object::Layer(layer.clone()))
            .await?;
        Ok(SyncLayerResult::Synced { layer, result })
    }

    pub async fn sync_manifest(&self, manifest: graph::Manifest) -> Result<SyncManifestResult> {
        let manifest_digest = manifest.digest()?;
        if self.skip_existing_objects && self.dest.has_manifest(manifest_digest).await {
            tracing::info!(digest = ?manifest_digest, "manifest already synced");
            return Ok(SyncManifestResult::Skipped);
        }

        tracing::debug!(digest = ?manifest_digest, "syncing manifest");
        let entries: Vec<_> = manifest
            .list_entries()
            .into_iter()
            .cloned()
            .filter(|e| e.kind.is_blob())
            .collect();
        let style = indicatif::ProgressStyle::default_bar()
            .template("      {msg} [{bar:40}] {bytes:>7}/{total_bytes:7}")
            .progress_chars("=>-");
        let total_bytes = entries.iter().fold(0, |c, e| c + e.size);
        let bar = indicatif::ProgressBar::new(total_bytes).with_style(style);
        bar.set_message("syncing manifest");
        let mut results = Vec::with_capacity(entries.len());
        let mut futures = futures::stream::FuturesUnordered::new();
        for entry in entries {
            futures.push(self.sync_entry(entry));
            while futures.len() >= MAX_CONCURRENT {
                if let Some(res) = futures.try_next().await? {
                    bar.inc(res.summary().synced_payload_bytes);
                    results.push(res);
                }
            }
        }
        while let Some(res) = futures.try_next().await? {
            bar.inc(res.summary().synced_payload_bytes);
            results.push(res);
        }
        bar.finish();

        self.dest
            .write_object(&graph::Object::Manifest(manifest.clone()))
            .await?;

        Ok(SyncManifestResult::Synced { manifest, results })
    }

    async fn sync_entry(&self, entry: graph::Entry) -> Result<SyncEntryResult> {
        if !entry.kind.is_blob() {
            return Ok(SyncEntryResult::Skipped);
        }
        let blob = graph::Blob {
            payload: entry.object,
            size: entry.size,
        };
        let result = self.sync_blob(blob).await?;
        Ok(SyncEntryResult::Synced { entry, result })
    }

    async fn sync_blob(&self, blob: graph::Blob) -> Result<SyncBlobResult> {
        if self.skip_existing_objects && self.dest.has_blob(blob.digest()).await {
            tracing::trace!(digest = ?blob.payload, "blob already synced");
            return Ok(SyncBlobResult::Skipped);
        }
        let result = self.sync_payload(blob.payload).await?;
        self.dest.write_blob(blob.clone()).await?;
        Ok(SyncBlobResult::Synced { blob, result })
    }

    async fn sync_payload(&self, digest: encoding::Digest) -> Result<SyncPayloadResult> {
        if self.skip_existing_payloads && self.dest.has_payload(digest).await {
            tracing::trace!(?digest, "blob payload already synced");
            return Ok(SyncPayloadResult::Skipped);
        }

        let payload = self.src.open_payload(digest).await?;
        tracing::debug!(?digest, "syncing payload");
        let (created_digest, size) = self.dest.write_data(payload).await?;
        if digest != created_digest {
            return Err(Error::String(format!(
                "Source repository provided blob that did not match the requested digest: wanted {digest}, got {created_digest}",
            )));
        }
        Ok(SyncPayloadResult::Synced { size })
    }
}

#[derive(Default, Debug)]
pub struct SyncSummary {
    pub skipped_tags: usize,
    pub synced_tags: usize,
    pub skipped_objects: usize,
    pub synced_objects: usize,
    pub skipped_payloads: usize,
    pub synced_payloads: usize,
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

pub struct SyncEnvResult {
    pub env: tracking::EnvSpec,
    pub results: Vec<SyncEnvItemResult>,
}

impl SyncEnvResult {
    pub fn summary(&self) -> SyncSummary {
        self.results.iter().map(|r| r.summary()).sum()
    }
}

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

pub enum SyncTagResult {
    /// The tag did not need to be synced
    Skipped,
    /// The tag was synced
    Synced {
        tag: tracking::TagSpec,
        result: SyncObjectResult,
    },
}

impl SyncTagResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped => SyncSummary {
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

pub enum SyncObjectResult {
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
            Platform(res) => res.summary(),
            Layer(res) => res.summary(),
            Blob(res) => res.summary(),
            Manifest(res) => res.summary(),
            Mask | Tree(_) => SyncSummary::default(),
        }
    }
}

pub enum SyncPlatformResult {
    /// The platform did not need to be synced
    Skipped,
    /// The platform was at least partially synced
    Synced {
        platform: graph::Platform,
        results: Vec<SyncObjectResult>,
    },
}

impl SyncPlatformResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped => SyncSummary::skipped_one_object(),
            Self::Synced { results, .. } => {
                let mut summary = results.iter().map(|r| r.summary()).sum();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

pub enum SyncLayerResult {
    /// The layer did not need to be synced
    Skipped,
    /// The layer was synced
    Synced {
        layer: graph::Layer,
        result: SyncManifestResult,
    },
}

impl SyncLayerResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped => SyncSummary::skipped_one_object(),
            Self::Synced { result, .. } => {
                let mut summary = result.summary();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

pub enum SyncManifestResult {
    /// The manifest did not need to be synced
    Skipped,
    /// The manifest was at least partially synced
    Synced {
        manifest: graph::Manifest,
        results: Vec<SyncEntryResult>,
    },
}

impl SyncManifestResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped => SyncSummary::skipped_one_object(),
            Self::Synced { results, .. } => {
                let mut summary = results.iter().map(|r| r.summary()).sum();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

pub enum SyncEntryResult {
    /// The entry did not need to be synced
    Skipped,
    /// The entry was synced
    Synced {
        entry: graph::Entry,
        result: SyncBlobResult,
    },
}

impl SyncEntryResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped => SyncSummary::skipped_one_object(),
            Self::Synced { result, .. } => {
                let mut summary = result.summary();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

pub enum SyncBlobResult {
    /// The blob did not need to be synced
    Skipped,
    /// The blob was synced
    Synced {
        blob: graph::Blob,
        result: SyncPayloadResult,
    },
}

impl SyncBlobResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped => SyncSummary::skipped_one_object(),
            Self::Synced { result, .. } => {
                let mut summary = result.summary();
                summary += SyncSummary::synced_one_object();
                summary
            }
        }
    }
}

pub enum SyncPayloadResult {
    /// The payload did not need to be synced
    Skipped,
    /// The payload was synced
    Synced { size: u64 },
}

impl SyncPayloadResult {
    pub fn summary(&self) -> SyncSummary {
        match self {
            Self::Skipped => SyncSummary {
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
