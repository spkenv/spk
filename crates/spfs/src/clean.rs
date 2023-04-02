// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::future::ready;
use std::os::linux::fs::MetadataExt;

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
    tag_stream_concurrency: usize,
    removal_concurrency: usize,
    discover_concurrency: usize,
    attached: DashSet<encoding::Digest>,
    dry_run: bool,
    must_be_older_than: DateTime<Utc>,
    prune_repeated_tags: bool,
    prune_params: PruneParameters,
    remove_proxies_with_no_links: bool,
}

impl<'repo> Cleaner<'repo, SilentCleanReporter> {
    /// See [`Cleaner::with_removal_concurrency`]
    pub const DEFAULT_REMOVAL_CONCURRENCY: usize = 500;
    /// See [`Cleaner::with_discover_concurrency`]
    pub const DEFAULT_DISCOVER_CONCURRENCY: usize = 50;
    /// See [`Cleaner::with_tag_stream_concurrency`]
    pub const DEFAULT_TAG_STREAM_CONCURRENCY: usize = 500;

    pub fn new(repo: &'repo storage::RepositoryHandle) -> Self {
        Self {
            repo,
            reporter: SilentCleanReporter,
            removal_concurrency: Self::DEFAULT_REMOVAL_CONCURRENCY,
            discover_concurrency: Self::DEFAULT_DISCOVER_CONCURRENCY,
            tag_stream_concurrency: Self::DEFAULT_TAG_STREAM_CONCURRENCY,
            attached: Default::default(),
            dry_run: false,
            must_be_older_than: Utc::now(),
            prune_repeated_tags: false,
            prune_params: Default::default(),
            remove_proxies_with_no_links: true,
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
            removal_concurrency: self.removal_concurrency,
            discover_concurrency: self.discover_concurrency,
            tag_stream_concurrency: self.tag_stream_concurrency,
            remove_proxies_with_no_links: self.remove_proxies_with_no_links,
        }
    }

    // The number of concurrent tag stream scanning operations
    // that are buffered and allowed to run concurrently
    pub fn with_tag_stream_concurrency(mut self, tag_stream_concurrency: usize) -> Self {
        self.tag_stream_concurrency = tag_stream_concurrency;
        self
    }

    // The number of concurrent remove operations that are
    // buffered and allowed to run concurrently
    pub fn with_removal_concurrency(mut self, removal_concurrency: usize) -> Self {
        self.removal_concurrency = removal_concurrency;
        self
    }

    // The number of concurrent discover/scan operations that are
    // buffered and allowed to run concurrently.
    //
    // This number is applied in a recursive manner, and so can grow
    // exponentially in deeply complex repositories.
    pub fn with_discover_concurrency(mut self, discover_concurrency: usize) -> Self {
        self.discover_concurrency = discover_concurrency;
        self
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

    /// When set, also remove any proxies that do not have any hard links
    /// regardless of if they are still attached in the repository.
    ///
    /// The lack of hard links shows that the proxy is not a part of any
    /// render for that particular user, and so is safe to remove. This removal
    /// still requires that the proxy is older than the configured limit to
    /// help reduce the chance of removing a proxy that was just created for
    /// a new render.
    pub fn with_remove_proxies_with_no_links(mut self, remove_proxies_with_no_links: bool) -> Self {
        self.remove_proxies_with_no_links = remove_proxies_with_no_links;
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
        let mut stream = self.repo.iter_tag_streams().boxed();
        let mut futures = futures::stream::FuturesUnordered::new();
        while let Some((tag_spec, _stream)) = stream.try_next().await? {
            if futures.len() > self.tag_stream_concurrency {
                // if we've reached the limit, let the fastest half finish
                // before adding additional futures. This is a crude way to
                // try and maximize parallel processing while also not leaving
                // completed futures for too long or needing to wait for the
                // slowest ones too often
                while futures.len() > self.tag_stream_concurrency / 2 {
                    futures.try_next().await?;
                }
            }
            futures.push(self.prune_tag_stream_and_walk(tag_spec));
        }
        drop(stream);
        while let Some(r) = futures.try_next().await? {
            result += r;
        }
        // because we don't yet know if some detached objects will be
        // kept due to age, we cannot process these two steps in parallel
        result += self.remove_unvisited_objects_and_payloads().await?;
        result += self.remove_unvisited_renders_and_proxies().await?;
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

        let mut result = CleanResult {
            visited_tags: to_prune.len() as u64 + to_keep.len() as u64,
            ..CleanResult::default()
        };

        for tag in to_prune.iter() {
            if !self.dry_run {
                self.repo.remove_tag(tag).await?;
            }
            self.reporter.tag_removed(tag);
        }

        result.pruned_tags.insert(tag_spec, to_prune);

        let mut walk_stream = futures::stream::iter(to_keep.iter())
            .then(|tag| ready(self.discover_attached_objects(tag.target).boxed()))
            .buffer_unordered(self.discover_concurrency)
            .boxed();
        while let Some(res) = walk_stream.try_next().await? {
            result += res;
        }

        Ok(result)
    }

    #[async_recursion::async_recursion]
    async fn discover_attached_objects(&self, digest: encoding::Digest) -> Result<CleanResult> {
        let mut result = CleanResult::default();
        if !self.attached.insert(digest) {
            return Ok(result);
        }

        let obj = match self.repo.read_object(digest).await {
            Ok(obj) => obj,
            Err(Error::UnknownObject(_)) => {
                // TODO: it would be nice to have an option to prune
                // broken tags that cause this error
                return Ok(result);
            }
            Err(err) => {
                self.reporter.error_encountered(&err);
                result.errors.push(err);
                return Ok(result);
            }
        };
        self.reporter.visit_object(&obj);
        result.visited_objects += 1;
        if let graph::Object::Blob(b) = &obj {
            result.visited_payloads += 1;
            self.reporter.visit_payload(b);
        }
        let mut walk_stream = futures::stream::iter(obj.child_objects())
            .then(|child| ready(self.discover_attached_objects(child).boxed()))
            .buffer_unordered(self.discover_concurrency)
            .boxed();
        while let Some(res) = walk_stream.try_next().await? {
            result += res;
        }
        Ok(result)
    }

    async fn remove_unvisited_objects_and_payloads(&self) -> Result<CleanResult> {
        let mut result = CleanResult::default();
        let mut stream = self
            .repo
            .iter_objects()
            // we have no interest in removing attached items
            .try_filter(|(digest, _object)| ready(!self.attached.contains(digest)))
            // we have already visited all attached objects
            // but also want to report these ones
            .and_then(|obj| {
                self.reporter.visit_object(&obj.1);
                result.visited_objects += 1;
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
            .try_buffer_unordered(self.removal_concurrency)
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
                result.visited_payloads += 1;
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
            .try_buffer_unordered(self.removal_concurrency)
            .boxed();
        let mut result = CleanResult::default();
        while let Some(blob) = stream.try_next().await? {
            result.removed_payloads.insert(blob.payload);
            self.reporter.payload_removed(&blob)
        }
        drop(stream);

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
                result.visited_payloads += 1;
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
            .try_buffer_unordered(self.removal_concurrency)
            .boxed();
        while let Some(blob) = stream.try_next().await? {
            result.removed_payloads.insert(blob.payload);
            self.reporter.payload_removed(&blob)
        }
        drop(stream);

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
        let mut result = CleanResult::default();
        let mut stream = repo
            .iter_rendered_manifests()
            .try_filter_map(|digest| {
                self.reporter.visit_render(&digest);
                result.visited_renders += 1;
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
            .try_buffer_unordered(self.removal_concurrency)
            .try_filter_map(|(digest, removed)| ready(Ok(removed.then_some(digest))))
            .boxed();
        let removed_for_user = result.removed_renders.entry(username.clone()).or_default();
        while let Some(digest) = stream.try_next().await? {
            removed_for_user.insert(digest);
            self.reporter.render_removed(&digest);
        }
        drop(stream);

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
        let mut result = CleanResult::default();
        let removed = result.removed_proxies.entry(username).or_default();

        // the hash store is used to nicely iterate all stored digests
        let proxy_storage = storage::fs::FSHashStore::open_unchecked(proxy_path);
        let mut stream = proxy_storage
            .iter()
            .try_filter_map(|digest| {
                self.reporter.visit_proxy(&digest);
                result.visited_proxies += 1;
                let path = proxy_storage.build_digest_path(&digest);
                async move {
                    if !self.attached.contains(&digest) {
                        // a detached object is always cleaned
                        return Ok(Some(digest));
                    }

                    if !self.remove_proxies_with_no_links {
                        // nothing else to check when this is disabled
                        return Ok(None);
                    }

                    let meta = tokio::fs::symlink_metadata(&path).await.map_err(|err| {
                        Error::StorageReadError("metadata on proxy file", path.clone(), err)
                    })?;
                    let mtime = meta.modified().map_err(|err| {
                        Error::StorageReadError("modified time on proxy file", path.clone(), err)
                    })?;
                    let has_hardlinks = meta.st_nlink() > 1;
                    let is_old_enough = DateTime::<Utc>::from(mtime) < self.must_be_older_than;
                    if has_hardlinks || !is_old_enough {
                        Ok(None)
                    } else {
                        Ok(Some(digest))
                    }
                }
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
            .try_buffer_unordered(self.removal_concurrency)
            .boxed();

        while let Some(res) = stream.next().await {
            match res {
                Ok(digest) => {
                    self.reporter.proxy_removed(&digest);
                    removed.insert(digest);
                }
                Err(err) => {
                    self.reporter.error_encountered(&err);
                    result.errors.push(err);
                }
            }
        }
        drop(stream);

        Ok(result)
    }
}

#[derive(Debug, Default)]
pub struct CleanResult {
    /// The number of tags visited when walking the database
    pub visited_tags: u64,
    /// The tags pruned from the database
    pub pruned_tags: HashMap<tracking::TagSpec, Vec<tracking::Tag>>,

    /// The number of objects visited when walking the database
    pub visited_objects: u64,
    /// The objects removed
    pub removed_objects: HashSet<encoding::Digest>,

    /// The number of payloads visited when walking the database
    pub visited_payloads: u64,
    /// The payloads removed
    pub removed_payloads: HashSet<encoding::Digest>,

    /// The number of renders visited when walking the database
    pub visited_renders: u64,
    /// The renders removed (by associated username)
    pub removed_renders: HashMap<Option<String>, HashSet<encoding::Digest>>,

    /// The number of proxies visited when walking the database
    pub visited_proxies: u64,
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
        // destructure to ensure that newly added fields
        // are accounted for in this function
        let CleanResult {
            visited_tags,
            pruned_tags,
            visited_objects,
            removed_objects,
            visited_payloads,
            removed_payloads,
            visited_renders,
            removed_renders,
            visited_proxies,
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
        self.visited_tags += visited_tags;
        self.visited_objects += visited_objects;
        self.removed_objects.extend(removed_objects);
        self.visited_payloads += visited_payloads;
        self.removed_payloads.extend(removed_payloads);
        self.visited_renders += visited_renders;
        self.visited_proxies += visited_proxies;
        self.errors.extend(errors);
    }
}

pub trait CleanReporter: Send + Sync {
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

    /// Called when a non-fatal error is encountered while cleaning
    fn error_encountered(&self, _err: &Error) {}
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

    fn error_encountered(&self, err: &Error) {
        tracing::error!(%err);
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
        self.get_bars().tags.inc(1);
    }

    fn tag_removed(&self, _tag: &tracking::Tag) {
        self.get_bars().tags.inc_length(1);
    }

    fn visit_object(&self, _object: &graph::Object) {
        self.get_bars().objects.inc(1);
    }

    fn object_removed(&self, _object: &graph::Object) {
        self.get_bars().objects.inc_length(1);
    }

    fn visit_payload(&self, _payload: &graph::Blob) {
        self.get_bars().payloads.inc(1);
    }

    fn payload_removed(&self, _payload: &graph::Blob) {
        self.get_bars().payloads.inc_length(1);
    }

    fn visit_proxy(&self, _proxy: &encoding::Digest) {
        self.get_bars().proxies.inc(1);
    }

    fn proxy_removed(&self, _proxy: &encoding::Digest) {
        self.get_bars().proxies.inc_length(1);
    }

    fn visit_render(&self, _render: &encoding::Digest) {
        self.get_bars().renders.inc(1);
    }

    fn render_removed(&self, _render: &encoding::Digest) {
        self.get_bars().renders.inc_length(1);
    }

    fn error_encountered(&self, err: &Error) {
        let msg = err.to_string().red().to_string();
        self.get_bars().tags.println(msg);
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
                " {spinner} {msg:<17.green} {pos:>10.cyan} found {len:>10.yellow} to remove [{per_sec}]",
            )
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
                .with_style(counter_style.clone())
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
        self.payloads.finish_and_clear();
        self.proxies.finish_and_clear();
        self.renders.finish_and_clear();
        self.objects.finish_and_clear();
        self.tags.finish_and_clear();
        if let Some(r) = self.renderer.take() {
            let _ = r.join();
        }
    }
}
