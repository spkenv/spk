// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::future::ready;

use chrono::{DateTime, Duration, Local, Utc};
use colored::Colorize;
use dashmap::DashSet;
use futures::{FutureExt, StreamExt, TryFutureExt, TryStreamExt};
use once_cell::sync::OnceCell;

use super::prune::PruneParameters;
use crate::runtime::makedirs_with_perms;
use crate::storage::fs::FSRepository;
use crate::{encoding, graph, storage, tracking, Error, Result};

#[cfg(test)]
#[path = "./clean_test.rs"]
mod clean_test;

const MAX_CONCURRENT_TAG_STREAMS: usize = 50;
const MAX_CONCURRENT_REMOVALS: usize = 50;

/// Runs a cleaning operation on a repository.
///
/// Primarily, this operation looks to remove data
/// which cannot be reached via a tag. Additional
/// parameters can be given to remove old and redundant
/// tags to free further objects to be removed.
pub struct Cleaner<'repo, Reporter = SilentCleanReporter>
where
    Reporter: CleanReporter,
{
    repo: &'repo storage::RepositoryHandle,
    reporter: Reporter,
    attached: DashSet<encoding::Digest>,
    dry_run: bool,
    must_be_older_than: DateTime<Utc>,
    prune_repeated_tags: bool,
    prune_params: PruneParameters,
}

impl<'repo> Cleaner<'repo, SilentCleanReporter> {
    pub fn new(repo: &'repo storage::RepositoryHandle) -> Self {
        Self {
            repo,
            reporter: SilentCleanReporter,
            attached: Default::default(),
            dry_run: false,
            must_be_older_than: Utc::now(),
            prune_repeated_tags: false,
            prune_params: Default::default(),
        }
    }
}

impl<'repo, Reporter> Cleaner<'repo, Reporter>
where
    Reporter: CleanReporter + Send + Sync,
{
    /// Report all progress to the given instance, replacing
    /// and existing reporter.
    pub fn with_reporter<R: CleanReporter>(self, reporter: R) -> Cleaner<'repo, R> {
        Cleaner {
            repo: self.repo,
            reporter,
            attached: self.attached,
            dry_run: self.dry_run,
            must_be_older_than: self.must_be_older_than,
            prune_repeated_tags: self.prune_repeated_tags,
            prune_params: self.prune_params,
        }
    }

    /// When dry run is enabled, the clean process doesn't actually
    /// remove any data, but otherwise operates as normal and reports on
    /// the decisions that would be made and data that would be removed
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Objects that have been in the repository for less
    /// than this amount of time are never removed, even
    /// if they are detached from a tag.
    ///
    /// This helps to ensure that active commit operations
    /// and regular write operations are not corrupted.
    ///
    /// Overrides and previous value for [`Self::with_required_age_cutoff`]
    pub fn with_required_age(mut self, age: Duration) -> Self {
        self.must_be_older_than = Utc::now() - age;
        self
    }

    /// Objects that are newer than this amount of time
    /// will never be removed.
    ///
    /// This helps to ensure that active commit operations
    /// and regular write operations are not corrupted.
    ///
    /// Overrides and previous value for [`Self::with_required_age`]
    pub fn with_required_age_cutoff(mut self, cutoff: DateTime<Utc>) -> Self {
        self.must_be_older_than = cutoff;
        self
    }

    /// When walking the history of a tag, delete older entries
    /// that have the same target as a more recent one.
    pub fn with_prune_repeated_tags(mut self, prune_repeated_tags: bool) -> Self {
        self.prune_repeated_tags = prune_repeated_tags;
        self
    }

    /// When walking the history of a tag, delete entries
    /// older than this date time
    pub fn with_prune_tags_older_than(
        mut self,
        prune_if_older_than: Option<DateTime<Utc>>,
    ) -> Self {
        self.prune_params.prune_if_older_than = prune_if_older_than;
        self
    }

    /// When walking the history of a tag, never delete entries
    /// newer than this date time
    pub fn with_keep_tags_newer_than(mut self, keep_if_newer_than: Option<DateTime<Utc>>) -> Self {
        self.prune_params.keep_if_newer_than = keep_if_newer_than;
        self
    }

    /// When walking the history of a tag, delete all additional
    /// entries leaving this number in the stream
    pub fn with_prune_tags_if_version_more_than(
        mut self,
        prune_if_version_more_than: Option<u64>,
    ) -> Self {
        self.prune_params.prune_if_version_more_than = prune_if_version_more_than;
        self
    }

    /// When walking the history of a tag, never leave less than
    /// this number of tags in the stream, regardless of other settings
    pub fn with_keep_tags_if_version_less_than(
        mut self,
        keep_if_version_less_than: Option<u64>,
    ) -> Self {
        self.prune_params.keep_if_version_less_than = keep_if_version_less_than;
        self
    }

    /// Provide a human-readable summary of the current
    /// configuration for this cleaner.
    ///
    /// This plan is intended to serve as a resource
    /// for confirming that the current configuration is
    /// setup as desired.
    pub fn format_plan(&self) -> String {
        let find = "FIND".cyan();
        let scan = "SCAN".cyan();
        let remove = "REMOVE".red();
        let prune = "PRUNE".yellow();
        let identify = "IDENTIFY".cyan();

        let mut out = format!("{}:\n", "Cleaning Plan".bold());
        let _ = writeln!(&mut out, "First, {scan} all of the tags in the repository.",);
        let _ = writeln!(
            &mut out,
            " - {} each item in the tag's history, and for each one:",
            "VISIT".cyan()
        );
        if self.prune_repeated_tags || !self.prune_params.is_empty() {
            if self.prune_repeated_tags {
                let _ = writeln!(
                    &mut out,
                    " - {prune} any entry that has a the same target as a more recent entry",
                );
            }
            let PruneParameters {
                prune_if_older_than,
                keep_if_newer_than,
                prune_if_version_more_than,
                keep_if_version_less_than,
            } = &self.prune_params;
            if let Some(dt) = prune_if_older_than {
                let _ = writeln!(
                    &mut out,
                    " - {identify} any tags older than {}",
                    dt.with_timezone(&Local)
                );
            }
            if let Some(v) = prune_if_version_more_than {
                let _ = writeln!(&mut out, " - {identify} any tags greater than version {v}",);
            }
            if keep_if_newer_than.is_some() || keep_if_version_less_than.is_some() {
                let _ = writeln!(&mut out, "{prune} the identified tags unless:");
                if let Some(dt) = keep_if_newer_than {
                    let _ = writeln!(
                        &mut out,
                        " - the tag was created after {}",
                        dt.with_timezone(&Local)
                    );
                }
                if let Some(v) = keep_if_version_less_than {
                    let _ = writeln!(&mut out, " - the tag's version is less than {v}",);
                }
            }
            let _ = writeln!(
                &mut out,
                " - otherwise, {find} all the objects and payloads connected to it",
            );
        } else {
            let _ = writeln!(
                &mut out,
                " - {find} all the objects and payloads connected to it",
            );
        }

        let _ = writeln!(
            &mut out,
            "Then, {scan} all of the objects in the repository"
        );
        let _ = writeln!(
            &mut out,
            " - {identify} any object that is not connected to a tag"
        );
        let _ = writeln!(
            &mut out,
            " - {remove} that object unless it was created after {}",
            self.must_be_older_than.with_timezone(&Local)
        );
        let _ = writeln!(
            &mut out,
            "Then, {scan} all of the payloads in the repository"
        );
        let _ = writeln!(
            &mut out,
            " - {remove} any payload that is not connected to a blob"
        );
        let _ = writeln!(
            &mut out,
            "Then, {scan} all of the renders in the repository"
        );
        let _ = writeln!(
            &mut out,
            " - {remove} any render that is not connected to an object"
        );
        out
    }

    /// Visit all tags, pruning as configured and then cleaning detached objects
    pub async fn prune_all_tags_and_clean(&self) -> Result<CleanResult> {
        let mut result = CleanResult::default();
        let mut stream = self.repo.iter_tag_streams();
        let mut futures = futures::stream::FuturesUnordered::new();
        while let Some((tag_spec, _stream)) = stream.try_next().await? {
            if futures.len() > MAX_CONCURRENT_TAG_STREAMS {
                // if we've reached the limit, let the fastest half finish
                // before adding additional futures. This is a crude way to
                // try and maximize parallel processing while also not leaving
                // completed futures for too long or needing to wait for the
                // slowest ones too often
                while futures.len() > MAX_CONCURRENT_TAG_STREAMS / 2 {
                    futures.try_next().await?;
                }
            }
            futures.push(self.prune_tag_stream_and_walk(tag_spec));
        }
        while let Some(r) = futures.try_next().await? {
            result += r;
        }
        let (r1, _) = tokio::try_join!(
            self.remove_unvisited_objects_and_payloads(),
            self.remove_unvisited_renders_and_proxies(),
        )?;
        result += r1;
        Ok(result)
    }

    async fn prune_tag_stream_and_walk(&self, tag_spec: tracking::TagSpec) -> Result<CleanResult> {
        let history = self
            .repo
            .read_tag(&tag_spec)
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        let mut to_prune = Vec::with_capacity(history.len() / 2);
        let mut to_keep = Vec::with_capacity(history.len() / 2);
        let mut seen_targets = std::collections::HashSet::new();
        for (i, tag) in history.into_iter().enumerate() {
            let spec = tag.to_spec(i as u64);
            self.reporter.visit_tag(&tag);
            if !seen_targets.insert(tag.target) && self.prune_repeated_tags {
                to_prune.push(tag);
                continue;
            }
            if self.prune_params.should_prune(&spec, &tag) {
                to_prune.push(tag);
            } else {
                to_keep.push(tag);
            }
        }

        for tag in to_prune.iter() {
            if !self.dry_run {
                self.repo.remove_tag(tag).await?;
            }
            self.reporter.tag_removed(tag);
        }

        for tag in to_keep {
            self.discover_attached_objects(tag.target).await?;
        }

        let mut result = CleanResult::default();
        result.pruned_tags.insert(tag_spec, to_prune);

        Ok(result)
    }

    #[async_recursion::async_recursion]
    async fn discover_attached_objects(&self, digest: encoding::Digest) -> Result<()> {
        if !self.attached.insert(digest) {
            return Ok(());
        }

        let obj = match self.repo.read_object(digest).await {
            Ok(obj) => obj,
            Err(Error::UnknownObject(_)) => {
                // TODO: it would be nice to have an option to prune
                // broken tags that cause this error
                return Ok(());
            }
            Err(err) => return Err(err),
        };
        self.reporter.visit_object(&obj);
        if let graph::Object::Blob(b) = &obj {
            self.reporter.visit_payload(b);
        }
        for child in obj.child_objects() {
            self.discover_attached_objects(child).await?;
        }
        Ok(())
    }

    async fn remove_unvisited_objects_and_payloads(&self) -> Result<CleanResult> {
        let mut stream = self
            .repo
            .iter_objects()
            // we have no interest in removing attached items
            .try_filter(|(digest, _object)| ready(!self.attached.contains(digest)))
            // we have already visited all attached objects
            // but also want to report these ones
            .and_then(|obj| {
                self.reporter.visit_object(&obj.1);
                ready(Ok(obj))
            })
            .and_then(|(digest, object)| {
                if self.dry_run {
                    return ready(Ok(ready(Ok((digest, object, true))).boxed()));
                }
                let future = self
                    .repo
                    .remove_object_if_older_than(self.must_be_older_than, digest)
                    .map(|res| {
                        if let Err(Error::UnknownObject(_)) = res {
                            return Ok(true);
                        }
                        res
                    })
                    .map_ok(move |removed| (digest, object, removed));
                ready(Ok(future.boxed()))
            })
            .try_buffer_unordered(MAX_CONCURRENT_REMOVALS)
            .try_filter_map(|(digest, obj, removed)| {
                if !removed {
                    // objects that are too new to be removed become
                    // implicitly attached
                    self.attached.insert(digest);
                }
                ready(Ok(removed.then_some(obj)))
            })
            .and_then(|obj| {
                self.reporter.object_removed(&obj);
                ready(Ok(obj))
            })
            // also try to remove the corresponding payload
            // each removed blob
            .try_filter_map(|obj| match obj {
                graph::Object::Blob(blob) => ready(Ok(Some(blob))),
                _ => ready(Ok(None)),
            })
            .and_then(|blob| {
                if self.dry_run {
                    return ready(Ok(ready(Ok(blob)).boxed()));
                }
                self.reporter.visit_payload(&blob);
                let future = self
                    .repo
                    .remove_payload(blob.payload)
                    .map(|res| {
                        if let Err(Error::UnknownObject(_)) = res {
                            return Ok(());
                        }
                        res
                    })
                    .map_ok(|_| blob);
                ready(Ok(future.boxed()))
            })
            .try_buffer_unordered(MAX_CONCURRENT_REMOVALS);
        let mut result = CleanResult::default();
        while let Some(blob) = stream.try_next().await? {
            result.removed_objects.insert(blob.payload);
            self.reporter.payload_removed(&blob)
        }

        let mut stream = self
            .repo
            .iter_payload_digests()
            .try_filter_map(|payload| {
                if self.attached.contains(&payload) {
                    return ready(Ok(None));
                }
                // TODO: this should be able to get the size of the payload, but
                // currently there is no way to do this unless you start with
                // the blob
                let blob = graph::Blob { payload, size: 0 };
                ready(Ok(Some(blob)))
            })
            .and_then(|blob| {
                self.reporter.visit_payload(&blob);
                if self.dry_run {
                    return ready(Ok(ready(Ok(blob)).boxed()));
                }
                let future = self
                    .repo
                    .remove_payload(blob.payload)
                    .map(|res| {
                        if let Err(Error::UnknownObject(_)) = res {
                            return Ok(());
                        }
                        res
                    })
                    .map_ok(|_| blob);
                ready(Ok(future.boxed()))
            })
            .try_buffer_unordered(MAX_CONCURRENT_REMOVALS);
        while let Some(blob) = stream.try_next().await? {
            result.removed_objects.insert(blob.payload);
            self.reporter.payload_removed(&blob)
        }

        Ok(result)
    }

    async fn remove_unvisited_renders_and_proxies(&self) -> Result<CleanResult> {
        let mut result = CleanResult::default();
        let storage::RepositoryHandle::FS(repo) = self.repo else {
            return Ok(result);
        };

        result += self
            .remove_unvisited_renders_and_proxies_for_storage(None, repo)
            .await?;

        let renders_for_all_users = repo.renders_for_all_users()?;

        for (username, sub_repo) in renders_for_all_users.iter() {
            result += self
                .remove_unvisited_renders_and_proxies_for_storage(Some(username.clone()), sub_repo)
                .await?;
        }
        Ok(result)
    }

    async fn remove_unvisited_renders_and_proxies_for_storage(
        &self,
        username: Option<String>,
        repo: &storage::fs::FSRepository,
    ) -> Result<CleanResult> {
        let mut stream = repo
            .iter_rendered_manifests()
            .try_filter_map(|digest| {
                self.reporter.visit_render(&digest);
                if self.attached.contains(&digest) {
                    return ready(Ok(None));
                }
                ready(Ok(Some(digest)))
            })
            .and_then(|digest| {
                if self.dry_run {
                    return ready(Ok(ready(Ok((digest, true))).boxed()));
                }
                let future = repo
                    .remove_rendered_manifest_if_older_than(self.must_be_older_than, digest)
                    .map(|res| {
                        if let Err(Error::UnknownObject(_)) = res {
                            return Ok(false);
                        }
                        res
                    })
                    .map_ok(move |removed| (digest, removed));
                ready(Ok(future.boxed()))
            })
            .try_buffer_unordered(MAX_CONCURRENT_REMOVALS)
            .try_filter_map(|(digest, removed)| ready(Ok(removed.then_some(digest))));
        let mut result = CleanResult::default();
        let removed_for_user = result.removed_renders.entry(username.clone()).or_default();
        while let Some(digest) = stream.try_next().await? {
            removed_for_user.insert(digest);
            self.reporter.render_removed(&digest);
        }

        if let Some(proxy_path) = repo.proxy_path() {
            result += self.clean_proxies(username, proxy_path.to_owned()).await?;
        }
        Ok(result)
    }

    /// Remove any unused proxy files.
    #[async_recursion::async_recursion]
    async fn clean_proxies(
        &self,
        username: Option<String>,
        proxy_path: std::path::PathBuf,
    ) -> Result<CleanResult> {
        // Any files in the proxy area that have a st_nlink count of 1 are unused
        // and can be removed.
        let mut result = CleanResult::default();
        let removed = result.removed_proxies.entry(username).or_default();

        // the hash store is used to nicely iterate all stored digests
        let proxy_storage = storage::fs::FSHashStore::open_unchecked(proxy_path);
        let mut stream = proxy_storage
            .iter()
            .try_filter_map(|digest| {
                self.reporter.visit_proxy(&digest);
                if self.attached.contains(&digest) {
                    return ready(Ok(None));
                }
                ready(Ok(Some(digest)))
            })
            .and_then(|digest| {
                let path = proxy_storage.build_digest_path(&digest);
                let workdir = proxy_storage.workdir();
                let _ = makedirs_with_perms(&workdir, proxy_storage.directory_permissions);
                let future = async move {
                    if !self.dry_run {
                        tracing::trace!(?path, "removing proxy render");
                        FSRepository::remove_dir_atomically(&path, &workdir).await?;
                    }
                    Ok(digest)
                };
                ready(Ok(future.boxed()))
            })
            // buffer/parallelize the removal operations
            .try_buffer_unordered(MAX_CONCURRENT_REMOVALS)
            .boxed();

        while let Some(res) = stream.next().await {
            tracing::warn!(?res);
            match res {
                Ok(digest) => {
                    self.reporter.proxy_removed(&digest);
                    removed.insert(digest);
                }
                Err(err) => result.errors.push(err),
            }
        }

        Ok(result)
    }
}

#[derive(Debug, Default)]
pub struct CleanResult {
    /// The tags pruned from the database
    pub pruned_tags: HashMap<tracking::TagSpec, Vec<tracking::Tag>>,
    /// The objects and payloads removed
    pub removed_objects: HashSet<encoding::Digest>,
    /// The renders removed (by associated username)
    pub removed_renders: HashMap<Option<String>, HashSet<encoding::Digest>>,
    /// The proxy payloads removed (by associated username)
    pub removed_proxies: HashMap<Option<String>, HashSet<encoding::Digest>>,
    /// Non-fatal errors encountered while cleaning.
    ///
    /// These are errors that stopped one or more items from
    /// being cleaned. For example, if the deletion of an object
    /// failed the error will be logged but the clean will continue
    pub errors: Vec<Error>,
}

impl CleanResult {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn into_all_tags(self) -> Vec<tracking::Tag> {
        self.pruned_tags.into_values().flatten().collect()
    }
}

impl std::ops::AddAssign for CleanResult {
    fn add_assign(&mut self, rhs: Self) {
        let CleanResult {
            pruned_tags,
            removed_objects,
            removed_renders,
            removed_proxies,
            errors,
        } = rhs;
        for (spec, tags) in pruned_tags {
            self.pruned_tags.entry(spec).or_default().extend(tags);
        }
        for (user, removed) in removed_proxies {
            self.removed_proxies
                .entry(user)
                .or_default()
                .extend(removed);
        }
        for (user, removed) in removed_renders {
            self.removed_renders
                .entry(user)
                .or_default()
                .extend(removed);
        }
        self.removed_objects.extend(removed_objects);
        self.errors.extend(errors);
    }
}

pub trait CleanReporter {
    /// Called when the cleaner visits a tag
    fn visit_tag(&self, _tag: &tracking::Tag) {}

    /// Called when a tag is pruned from the repository
    fn tag_removed(&self, _tag: &tracking::Tag) {}

    /// Called when the cleaner visits an object in the graph
    fn visit_object(&self, _object: &graph::Object) {}

    /// Called when the cleaner removes ao object from the database
    fn object_removed(&self, _object: &graph::Object) {}

    /// Called when the cleaner visits a payload during scanning
    fn visit_payload(&self, _payload: &graph::Blob) {}

    /// Called when the cleaner removes a payload from the database
    fn payload_removed(&self, _payload: &graph::Blob) {}

    /// Called when the cleaner visits a proxy during scanning
    fn visit_proxy(&self, _proxy: &encoding::Digest) {}

    /// Called when the cleaner removes a proxy from the database
    fn proxy_removed(&self, _proxy: &encoding::Digest) {}

    /// Called when the cleaner visits a rendered directory during scanning
    fn visit_render(&self, _render: &encoding::Digest) {}

    /// Called when the cleaner removes a render from disk
    fn render_removed(&self, _render: &encoding::Digest) {}
}

pub struct SilentCleanReporter;

impl CleanReporter for SilentCleanReporter {}

/// Logs all events using tracing
pub struct TracingCleanReporter;

impl CleanReporter for TracingCleanReporter {
    fn visit_tag(&self, tag: &tracking::Tag) {
        tracing::info!(?tag, "visit tag");
    }

    fn tag_removed(&self, tag: &tracking::Tag) {
        tracing::info!(?tag, "tag removed");
    }

    fn visit_object(&self, object: &graph::Object) {
        tracing::info!(?object, "visit object");
    }

    fn object_removed(&self, object: &graph::Object) {
        tracing::info!(?object, "object removed");
    }

    fn visit_payload(&self, payload: &graph::Blob) {
        tracing::info!(?payload, "visit payload");
    }

    fn payload_removed(&self, payload: &graph::Blob) {
        tracing::info!(?payload, "payload removed");
    }

    fn visit_proxy(&self, proxy: &encoding::Digest) {
        tracing::info!(?proxy, "visit proxy");
    }

    fn proxy_removed(&self, proxy: &encoding::Digest) {
        tracing::info!(?proxy, "proxy removed");
    }

    fn visit_render(&self, render: &encoding::Digest) {
        tracing::info!(?render, "visit render");
    }

    fn render_removed(&self, render: &encoding::Digest) {
        tracing::info!(?render, "render removed");
    }
}

/// Reports sync progress to an interactive console via progress bars
#[derive(Default)]
pub struct ConsoleCleanReporter {
    bars: OnceCell<ConsoleCleanReporterBars>,
}

impl ConsoleCleanReporter {
    fn get_bars(&self) -> &ConsoleCleanReporterBars {
        self.bars.get_or_init(Default::default)
    }
}

impl CleanReporter for ConsoleCleanReporter {
    fn visit_tag(&self, _tag: &tracking::Tag) {
        self.get_bars().tags.inc_length(1);
    }

    fn tag_removed(&self, _tag: &tracking::Tag) {
        self.get_bars().tags.inc(1);
    }

    fn visit_object(&self, _object: &graph::Object) {
        self.get_bars().objects.inc_length(1);
    }

    fn object_removed(&self, _object: &graph::Object) {
        self.get_bars().objects.inc(1);
    }

    fn visit_payload(&self, payload: &graph::Blob) {
        self.get_bars().payloads.inc_length(payload.size);
    }

    fn payload_removed(&self, payload: &graph::Blob) {
        self.get_bars().payloads.inc(payload.size);
    }

    fn visit_proxy(&self, _proxy: &encoding::Digest) {
        self.get_bars().proxies.inc_length(1);
    }

    fn proxy_removed(&self, _proxy: &encoding::Digest) {
        self.get_bars().proxies.inc(1);
    }

    fn visit_render(&self, _render: &encoding::Digest) {
        self.get_bars().renders.inc_length(1);
    }

    fn render_removed(&self, _render: &encoding::Digest) {
        self.get_bars().renders.inc(1);
    }
}

struct ConsoleCleanReporterBars {
    renderer: Option<std::thread::JoinHandle<()>>,
    tags: indicatif::ProgressBar,
    objects: indicatif::ProgressBar,
    renders: indicatif::ProgressBar,
    payloads: indicatif::ProgressBar,
    proxies: indicatif::ProgressBar,
}

impl Default for ConsoleCleanReporterBars {
    fn default() -> Self {
        static TICK_STRINGS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        static PROGRESS_CHARS: &str = "=>-";
        let counter_style = indicatif::ProgressStyle::default_bar()
            .template(
                " {spinner} {msg:<17.green} {len:>10.cyan} found {pos:>10.yellow} to remove ({percent}%)",
            )
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let bytes_style = indicatif::ProgressStyle::default_bar()
            .template(" {spinner} {msg:<17.green} {total_bytes:>10.cyan} found {bytes:>10.yellow} to remove ({percent}%)")
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let bars = indicatif::MultiProgress::new();
        let tags = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(counter_style.clone())
                .with_message("cleaning tags"),
        );
        let objects = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(counter_style.clone())
                .with_message("cleaning objects"),
        );
        let payloads = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(bytes_style)
                .with_message("cleaning payloads"),
        );
        let renders = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(counter_style.clone())
                .with_message("cleaning renders"),
        );
        let proxies = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(counter_style)
                .with_message("cleaning proxies"),
        );
        tags.enable_steady_tick(100);
        objects.enable_steady_tick(100);
        renders.enable_steady_tick(100);
        proxies.enable_steady_tick(100);
        payloads.enable_steady_tick(100);
        // the progress bar must be awaited from some thread
        // or nothing will be shown in the terminal
        let renderer = Some(std::thread::spawn(move || {
            if let Err(err) = bars.join() {
                tracing::error!("Failed to render clean progress: {err}");
            }
        }));
        Self {
            renderer,
            tags,
            objects,
            payloads,
            proxies,
            renders,
        }
    }
}

impl Drop for ConsoleCleanReporterBars {
    fn drop(&mut self) {
        self.payloads.finish_at_current_pos();
        self.proxies.finish_at_current_pos();
        self.renders.finish_at_current_pos();
        self.objects.finish_at_current_pos();
        self.tags.finish_at_current_pos();
        if let Some(r) = self.renderer.take() {
            let _ = r.join();
        }
    }
}
