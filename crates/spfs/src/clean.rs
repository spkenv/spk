// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fmt::Write;
use std::future::ready;
use std::os::linux::fs::MetadataExt;

use chrono::{DateTime, Duration, Local, Utc};
use colored::Colorize;
use dashmap::DashSet;
use futures::{FutureExt, TryFutureExt, TryStreamExt};
use once_cell::sync::OnceCell;

use super::prune::PruneParameters;
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
    pub fn with_required_age(mut self, age: Duration) -> Self {
        self.must_be_older_than = Utc::now() - age;
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

    /// Visit all tags, pruning as configured and then cleaning detatched objects
    pub async fn prune_all_tags_and_clean(&self) -> Result<()> {
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
        while let Some(_result) = futures.try_next().await? {}
        tokio::try_join!(
            self.remove_unvisited_objects_and_payloads(),
            self.remove_unvisited_renders(),
        )?;
        Ok(())
    }

    async fn prune_tag_stream_and_walk(&self, tag_spec: tracking::TagSpec) -> Result<()> {
        let history = self
            .repo
            .read_tag(&tag_spec)
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        let mut to_prune = Vec::with_capacity(history.len() / 2);
        let mut to_keep = Vec::with_capacity(history.len() / 2);
        let mut seen_targets = std::collections::HashSet::new();
        for tag in history {
            self.reporter.visit_tag(&tag);
            if !seen_targets.insert(tag.target) && self.prune_repeated_tags {
                to_prune.push(tag);
                continue;
            }
            if self.prune_params.should_prune(&tag_spec, &tag) {
                to_prune.push(tag);
            } else {
                to_keep.push(tag);
            }
        }

        for tag in to_prune {
            if !self.dry_run {
                self.repo.remove_tag(&tag).await?;
            }
            self.reporter.tag_removed(&tag);
        }

        for tag in to_keep {
            self.discover_attached_objects(tag.target).await?;
        }

        Ok(())
    }

    #[async_recursion::async_recursion]
    async fn discover_attached_objects(&self, digest: encoding::Digest) -> Result<()> {
        if !self.attached.insert(digest) {
            return Ok(());
        }

        let obj = self.repo.read_object(digest).await?;
        self.reporter.visit_object(&obj);
        if let graph::Object::Blob(b) = &obj {
            self.reporter.visit_payload(b);
        }
        for child in obj.child_objects() {
            self.discover_attached_objects(child).await?;
        }
        Ok(())
    }

    async fn remove_unvisited_objects_and_payloads(&self) -> Result<()> {
        let mut stream = self
            .repo
            .iter_objects()
            .try_filter(|(digest, _object)| ready(!self.attached.contains(digest)))
            .and_then(|obj| {
                self.reporter.visit_object(&obj.1);
                ready(Ok(obj))
            })
            .and_then(|(digest, object)| {
                let future = self
                    .repo
                    .remove_object_if_older_than(self.must_be_older_than, digest)
                    .map(|res| {
                        if let Err(Error::UnknownObject(_)) = res {
                            return Ok(true);
                        }
                        res
                    })
                    .map_ok(|removed| (object, removed));
                ready(Ok(future))
            })
            .try_buffer_unordered(MAX_CONCURRENT_REMOVALS)
            .try_filter_map(|(obj, removed)| ready(Ok(removed.then_some(obj))))
            .and_then(|obj| {
                self.reporter.object_removed(&obj);
                ready(Ok(obj))
            })
            .try_filter_map(|obj| match obj {
                graph::Object::Blob(blob) => ready(Ok(Some(blob))),
                _ => ready(Ok(None)),
            })
            .and_then(|blob| {
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
                ready(Ok(future))
            })
            .try_buffer_unordered(MAX_CONCURRENT_REMOVALS);
        while let Some(blob) = stream.try_next().await? {
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
                let future = self.repo.remove_payload(blob.payload).map_ok(|_| blob);
                ready(Ok(future))
            })
            .try_buffer_unordered(MAX_CONCURRENT_REMOVALS);
        while let Some(blob) = stream.try_next().await? {
            self.reporter.payload_removed(&blob)
        }

        Ok(())
    }

    async fn remove_unvisited_renders(&self) -> Result<()> {
        let storage::RepositoryHandle::FS(repo) = self.repo else {
            return Ok(());
        };

        self.remove_unvisited_renders_for_storage(repo).await?;

        let renders_for_all_users = repo.renders_for_all_users()?;

        for (_username, sub_repo) in renders_for_all_users.iter() {
            self.remove_unvisited_renders_for_storage(sub_repo).await?;
        }
        Ok(())
    }

    async fn remove_unvisited_renders_for_storage(
        &self,
        repo: &storage::fs::FSRepository,
    ) -> Result<()> {
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
                let future = repo
                    .remove_rendered_manifest_if_older_than(self.must_be_older_than, digest)
                    .map_ok(move |removed| (digest, removed));
                ready(Ok(future))
            })
            .try_buffer_unordered(MAX_CONCURRENT_REMOVALS)
            .try_filter_map(|(digest, removed)| ready(Ok(removed.then_some(digest))));
        while let Some(digest) = stream.try_next().await? {
            self.reporter.render_removed(&digest);
        }

        if let Some(proxy_path) = repo.proxy_path() {
            self.clean_proxy(proxy_path.to_owned()).await?;
        }
        Ok(())
    }

    /// Remove any unused proxy files.
    ///
    /// Return true if any files were deleted, or if an empty directory was found.
    #[async_recursion::async_recursion]
    async fn clean_proxy(&self, proxy_path: std::path::PathBuf) -> Result<bool> {
        // Any files in the proxy area that have a st_nlink count of 1 are unused
        // and can be removed.
        let mut files_exist = false;
        let mut files_were_deleted = false;
        let mut iter = tokio::fs::read_dir(&proxy_path).await.map_err(|err| {
            Error::StorageReadError("read_dir on proxy path", proxy_path.clone(), err)
        })?;
        while let Some(entry) = iter.next_entry().await.map_err(|err| {
            Error::StorageReadError("next_entry on proxy path", proxy_path.clone(), err)
        })? {
            files_exist = true;

            let file_type = entry.file_type().await.map_err(|err| {
                Error::StorageReadError("file_type on proxy path entry", entry.path(), err)
            })?;

            if file_type.is_dir() {
                if self.clean_proxy(entry.path()).await? {
                    // If some files were deleted, attempt to delete the directory
                    // itself. It may now be empty. Ignore any failures.
                    if self.dry_run {
                        tracing::debug!("rmdir {}", entry.path().display());
                    } else if (tokio::fs::remove_dir(entry.path()).await).is_ok() {
                        files_were_deleted = true;
                    }
                }
            } else if file_type.is_file() {
                let metadata = entry.metadata().await.map_err(|err| {
                    Error::StorageReadError("metadata on proxy file", entry.path(), err)
                })?;

                if metadata.st_nlink() != 1 {
                    continue;
                }

                // This file with st_nlink count of 1 is "safe" to remove. There
                // may be some other process that is about to create a hard link
                // to this file, and will fail if it goes missing.
                if self.dry_run {
                    tracing::debug!("rm {}", entry.path().display());
                } else {
                    tokio::fs::remove_file(entry.path()).await.map_err(|err| {
                        Error::StorageReadError(
                            "remove_file on proxy path entry",
                            entry.path(),
                            err,
                        )
                    })?;
                }

                files_were_deleted = true;
            }
        }
        Ok(files_were_deleted || !files_exist)
    }
}

pub trait CleanReporter {
    /// Called when the cleaner visits a tag
    fn visit_tag(&self, _tag: &tracking::Tag) {}

    /// Called when a tag is pruned from the repository
    fn tag_removed(&self, _tag: &tracking::Tag) {}

    /// Called when the cleaner visits an object in the graph
    fn visit_object(&self, _object: &graph::Object) {}

    /// Called when the cleaner removes an object from the database
    fn object_removed(&self, _object: &graph::Object) {}

    /// Called when the cleaner visits a payload during scanning
    fn visit_payload(&self, _payload: &graph::Blob) {}

    /// Called when the cleaner removes an payload from the database
    fn payload_removed(&self, _payload: &graph::Blob) {}

    /// Called when the cleaner visits a rendered directory during scanning
    fn visit_render(&self, _render: &encoding::Digest) {}

    /// Called when the cleaner removes an render from disk
    fn render_removed(&self, _render: &encoding::Digest) {}
}

pub struct SilentCleanReporter;

impl CleanReporter for SilentCleanReporter {}

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
}

impl Default for ConsoleCleanReporterBars {
    fn default() -> Self {
        static TICK_STRINGS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        static PROGRESS_CHARS: &str = "=>-";
        let tags_style = indicatif::ProgressStyle::default_bar()
            .template(
                "      {spinner} {msg:<17.green} {len:>10.cyan} found {pos:>10.yellow} to prune",
            )
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let objects_style = indicatif::ProgressStyle::default_bar()
            .template(
                "      {spinner} {msg:<17.green} {len:>10.cyan} found {pos:>10.yellow} to remove",
            )
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let bytes_style = indicatif::ProgressStyle::default_bar()
            .template("      {spinner} {msg:<17.green} {total_bytes:>10.cyan} found {bytes:>10.yellow} to remove")
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let renders_style = indicatif::ProgressStyle::default_bar()
            .template(
                "      {spinner} {msg:<17.green} {len:>10.cyan} found {pos:>10.yellow} to remove",
            )
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let bars = indicatif::MultiProgress::new();
        let tags = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(tags_style)
                .with_message("cleaning tags"),
        );
        let objects = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(objects_style)
                .with_message("cleaning objects"),
        );
        let payloads = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(bytes_style)
                .with_message("cleaning payloads"),
        );
        let renders = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(renders_style)
                .with_message("cleaning renders"),
        );
        tags.enable_steady_tick(100);
        objects.enable_steady_tick(100);
        renders.enable_steady_tick(100);
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
            renders,
        }
    }
}

impl Drop for ConsoleCleanReporterBars {
    fn drop(&mut self) {
        self.payloads.finish_at_current_pos();
        self.renders.finish_at_current_pos();
        self.objects.finish_at_current_pos();
        self.tags.finish_at_current_pos();
        if let Some(r) = self.renderer.take() {
            let _ = r.join();
        }
    }
}
