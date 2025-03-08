// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_stream::try_stream;
use futures::{Stream, StreamExt};
use relative_path::RelativePathBuf;
use spfs::encoding::Digest;
use spfs::graph::object::EncodingFormat;
use spfs::prelude::*;
use spfs::sync::reporter::SyncReporters;
use spfs::tracking::{Entry, EntryKind};
use spk_schema::Spec;
use spk_schema::foundation::format::{FormatIdent, FormatOptionMap};
use spk_schema::foundation::ident_component::Component;
use spk_schema::prelude::*;
use spk_solve::solution::{PackageSource, SPK_SOLVE_EXTRA_DATA_KEY, Solution};
use spk_solve::{BuildIdent, RepositoryHandle};
use spk_storage as storage;
use tokio::pin;

use crate::{Error, Result};

#[cfg(test)]
#[path = "./exec_test.rs"]
mod exec_test;

/// A pair of packages that are in conflict for some reason,
/// e.g. because they both provide one or more of the same files.
#[derive(Eq, Hash, PartialEq)]
pub struct ConflictingPackagePair(BuildIdent, BuildIdent);

/// A single layer of a resolved solution.
#[derive(Clone)]
pub struct ResolvedLayer {
    pub digest: Digest,
    pub spec: Arc<Spec>,
    pub component: Component,
    pub repo: Arc<RepositoryHandle>,
}

/// A stack of layers of a resolved solution.
#[derive(Clone)]
pub struct ResolvedLayers(Vec<ResolvedLayer>);

impl ResolvedLayers {
    /// Return a stream over all the file objects described by the resolved
    /// layers.
    pub fn iter_entries(
        &self,
    ) -> impl Stream<Item = Result<(RelativePathBuf, Entry, &ResolvedLayer)>> + '_ {
        use spfs::graph::object::Enum;
        try_stream! {
            for resolved_layer in self.0.iter() {
                let manifest = match &*resolved_layer.repo {
                    RepositoryHandle::SPFS(repo) => {
                        let object = repo.read_object(resolved_layer.digest).await?;
                        match object.into_enum() {
                            Enum::Layer(obj) => {
                                let manifest_digest = match obj.manifest() {
                                    None => continue,
                                    Some(m) => m
                                };
                                match repo.read_object(*manifest_digest).await?.into_enum() {
                                    Enum::Manifest(obj) => obj,
                                    _ => continue,
                                }
                            }
                            Enum::Manifest(obj) => obj,
                            _ => continue,
                        }
                    }
                    _ => Err(Error::NonSpfsLayerInResolvedLayers)?,
                };
                let unlock = manifest.to_tracking_manifest();
                let walker = unlock.walk();
                for node in walker {
                    yield (node.path, node.entry.clone(), resolved_layer)
                }
            }
        }
    }

    /// Return the resolved layers as a list of digests.
    pub fn layers(&self) -> Vec<Digest> {
        self.0.iter().map(|l| l.digest).collect()
    }

    /// Compute a [`spfs::tracking::Manifest`] from a [`ResolvedLayers`].
    ///
    /// If any shadowed files are detected a warning will be logged. Because the
    /// layers in a Solution are in an arbitrary order, the Manifest's contents
    /// can be unpredictable if multiple layers contain overlapping entries.
    pub async fn get_environment_filesystem(
        &self,
        ident: BuildIdent,
        conflicting_packages: &mut HashMap<ConflictingPackagePair, HashSet<RelativePathBuf>>,
    ) -> Result<spfs::tracking::Manifest<BuildIdent>> {
        let mut environment_filesystem = spfs::tracking::Manifest::new(
            // we expect this to be replaced, but the source build for this package
            // seems like one of the most reasonable default owners for the root
            // of this manifest until then
            spfs::tracking::Entry::empty_dir_with_open_perms_with_data(ident),
        );

        // Warn about possibly unexpected shadowed files in the layer stack.
        let mut warning_found = false;
        let entries = self.iter_entries();
        pin!(entries);

        while let Some(entry) = entries.next().await {
            let (path, entry, resolved_layer) = match entry {
                Err(Error::NonSpfsLayerInResolvedLayers) => continue,
                Err(err) => return Err(err),
                Ok(entry) => entry,
            };

            let mut entry = entry.and_user_data(resolved_layer.spec.ident().to_owned());
            let Some(previous) = environment_filesystem.get_path(&path).cloned() else {
                environment_filesystem.mknod(&path, entry)?;
                continue;
            };
            // If old and new entries are both Trees, then merge them to
            // properly mimic overlayfs behavior.
            if previous.kind == EntryKind::Tree && entry.kind == EntryKind::Tree {
                for (previous_entry_name, previous_entry) in previous.entries.into_iter() {
                    entry
                        .entries
                        .entry(previous_entry_name)
                        .or_insert(previous_entry);
                }
            }
            let entry = environment_filesystem.mknod(&path, entry)?;
            if !matches!(entry.kind, EntryKind::Blob(_)) {
                continue;
            }

            // Ignore when the shadowing is from different components
            // of the same package.
            if entry.user_data == previous.user_data {
                continue;
            }

            // The layer order isn't necessarily meaningful in terms
            // of spk package dependency ordering (at the time of
            // writing), so phrase this in a way that doesn't suggest
            // one layer "owns" the file more than the other.
            warning_found = true;
            tracing::warn!(
                "File {path} found in more than one package: {} and {}",
                previous.user_data,
                entry.user_data
            );

            // Track the packages involved for later use
            let pkg_a = previous.user_data.clone();
            let pkg_b = entry.user_data.clone();
            let packages_key = if pkg_a < pkg_b {
                ConflictingPackagePair(pkg_a, pkg_b)
            } else {
                ConflictingPackagePair(pkg_b, pkg_a)
            };
            let counter = conflicting_packages.entry(packages_key).or_default();
            counter.insert(path.clone());
        }
        if warning_found {
            tracing::warn!("Conflicting files were detected");
            tracing::warn!(" > This can cause undefined runtime behavior");
            tracing::warn!(" > It should be addressed by:");
            tracing::warn!("   - not using these packages together");
            tracing::warn!("   - removing the file from one of them");
            tracing::warn!("   - using alternate versions or components");
        }

        Ok(environment_filesystem)
    }
}

/// Return the necessary layers to have all solution packages.
pub fn solution_to_resolved_runtime_layers(solution: &Solution) -> Result<ResolvedLayers> {
    let mut seen = HashSet::new();
    let mut stack = Vec::new();

    for resolved in solution.items() {
        let (repo, components) = match &resolved.source {
            PackageSource::Repository { repo, components } => (repo, components),
            PackageSource::Embedded { .. } => continue,
            PackageSource::BuildFromSource { .. } => {
                // The resolved solution includes a package that needs
                // to be built with specific options because such a
                // build doesn't exist in a repo.
                let build_options = resolved.spec.option_values();
                return Err(Error::String(format!(
                    "Solution includes package that needs building from source: {} with these options: {}",
                    resolved.spec.ident().format_ident(),
                    build_options.format_option_map(),
                )));
            }
            PackageSource::SpkInternalTest => continue,
        };

        if resolved.request.pkg.components.is_empty() {
            tracing::warn!(
                "Package request for '{}' identified no components, nothing will be included",
                resolved.request.pkg.name
            );
            continue;
        }
        let mut desired_components = resolved.request.pkg.components.clone();
        if desired_components.is_empty() || desired_components.remove(&Component::All) {
            desired_components.extend(components.keys().cloned());
        }
        desired_components = resolved
            .spec
            .components()
            .resolve_uses(desired_components.iter());

        for name in desired_components.into_iter() {
            let digest = components.get(&name).ok_or_else(|| {
                Error::String(format!(
                    "Resolved component '{name}' went missing, this is likely a bug in the solver"
                ))
            })?;

            if seen.insert(*digest) {
                stack.push(ResolvedLayer {
                    digest: *digest,
                    spec: Arc::clone(&resolved.spec),
                    component: name,
                    repo: Arc::clone(repo),
                });
            }
        }
    }

    Ok(ResolvedLayers(stack))
}

/// List the necessary layers to have all solution packages, pulling them if
/// required by the given runtime.
///
/// The default Syncer reporter is used. See
/// ['resolve_runtime_layers_with_reporter'] to be able to customize the
/// reporter.
pub async fn resolve_runtime_layers(
    requires_localization: bool,
    solution: &Solution,
) -> Result<Vec<Digest>> {
    resolve_runtime_layers_with_reporter(requires_localization, solution, SyncReporters::console)
        .await
}

/// List the necessary layers to have all solution packages, pulling them if
/// required by the given runtime.
///
/// The Syncer reporter is customizable.
pub async fn resolve_runtime_layers_with_reporter<F>(
    requires_localization: bool,
    solution: &Solution,
    reporter: F,
) -> Result<Vec<Digest>>
where
    F: Fn() -> SyncReporters,
{
    let resolved = solution_to_resolved_runtime_layers(solution)?;
    if requires_localization {
        pull_resolved_runtime_layers_with_reporter(&resolved, reporter).await
    } else {
        Ok(resolved.layers())
    }
}

/// Pull and return the specified resolved layers.
///
/// The default Syncer reporter is used. See
/// [`pull_resolved_runtime_layers_with_reporter`] to be able to customize the
/// reporter.
pub async fn pull_resolved_runtime_layers(resolved_layers: &ResolvedLayers) -> Result<Vec<Digest>> {
    pull_resolved_runtime_layers_with_reporter(resolved_layers, SyncReporters::console).await
}

/// Pull and return the specified resolved layers.
///
/// The Syncer reporter is customizable.
pub async fn pull_resolved_runtime_layers_with_reporter<F>(
    resolved_layers: &ResolvedLayers,
    reporter: F,
) -> Result<Vec<Digest>>
where
    F: Fn() -> SyncReporters,
{
    let local_repo = storage::local_repository().await?;
    let mut stack = Vec::with_capacity(resolved_layers.0.len());
    let mut to_sync = Vec::new();

    for resolved_layer in resolved_layers.0.iter() {
        stack.push(resolved_layer.digest);

        if !local_repo.has_object(resolved_layer.digest).await {
            to_sync.push((
                Arc::clone(&resolved_layer.spec),
                Arc::clone(&resolved_layer.repo),
                resolved_layer.digest,
            ))
        }
    }

    let to_sync_count = to_sync.len();
    for (i, (spec, repo, digest)) in to_sync.into_iter().enumerate() {
        if let storage::RepositoryHandle::SPFS(repo) = &*repo {
            tracing::info!(
                "collecting {} of {} {}",
                i + 1,
                to_sync_count,
                spec.ident().format_ident(),
            );
            let syncer = spfs::Syncer::new(repo, &local_repo).with_reporter(reporter());
            syncer.sync_digest(digest).await?;
        }
    }

    Ok(stack)
}

/// Modify the active spfs runtime to include exactly the packages in the given solution.
pub async fn setup_current_runtime(solution: &Solution) -> Result<()> {
    let mut rt = spfs::active_runtime().await?;
    setup_runtime(&mut rt, solution).await
}

pub async fn setup_runtime(rt: &mut spfs::runtime::Runtime, solution: &Solution) -> Result<()> {
    setup_runtime_with_reporter(rt, solution, SyncReporters::console).await
}

pub async fn setup_runtime_with_reporter<F>(
    rt: &mut spfs::runtime::Runtime,
    solution: &Solution,
    reporter: F,
) -> Result<()>
where
    F: Fn() -> SyncReporters,
{
    let stack = resolve_runtime_layers_with_reporter(
        rt.config.mount_backend.requires_localization(),
        solution,
        reporter,
    )
    .await?;
    rt.status.stack = spfs::graph::Stack::from_iter(stack);

    let spfs_config = spfs::Config::current()?;
    // Annotations are only supported with FlatFileBuffers
    if spfs_config.storage.encoding_format == EncodingFormat::FlatBuffers {
        // Store additional solve data all the resolved packages as extra
        // data in the spfs runtime so future spk commands run inside the
        // runtime can access it.
        let solve_data = serde_json::to_string(&solution.packages_to_solve_data())
            .map_err(|err| Error::String(err.to_string()))?;
        rt.add_annotation(
            SPK_SOLVE_EXTRA_DATA_KEY,
            &solve_data,
            spfs_config.filesystem.annotation_size_limit,
        )
        .await?;
    }

    rt.save_state_to_storage().await?;
    spfs::remount_runtime(rt).await?;
    Ok(())
}
