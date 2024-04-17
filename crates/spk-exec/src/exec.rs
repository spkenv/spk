// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::sync::Arc;

use async_stream::try_stream;
use futures::Stream;
use relative_path::RelativePathBuf;
use spfs::encoding::Digest;
use spfs::prelude::*;
use spfs::tracking::Entry;
use spk_schema::foundation::format::{FormatIdent, FormatOptionMap};
use spk_schema::foundation::ident_component::Component;
use spk_schema::prelude::*;
use spk_schema::Spec;
use spk_solve::solution::{PackageSource, Solution, SPK_SOLVE_EXTRA_DATA_KEY};
use spk_solve::RepositoryHandle;
use spk_storage as storage;

use crate::{Error, Result};

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
pub async fn resolve_runtime_layers(
    requires_localization: bool,
    solution: &Solution,
) -> Result<Vec<Digest>> {
    let resolved = solution_to_resolved_runtime_layers(solution)?;
    if requires_localization {
        pull_resolved_runtime_layers(&resolved).await
    } else {
        Ok(resolved.layers())
    }
}

/// Pull and return the specified resolved layers.
pub async fn pull_resolved_runtime_layers(resolved_layers: &ResolvedLayers) -> Result<Vec<Digest>> {
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
            let syncer = spfs::Syncer::new(repo, &local_repo)
                .with_reporter(spfs::sync::ConsoleSyncReporter::default());
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
    let stack =
        resolve_runtime_layers(rt.config.mount_backend.requires_localization(), solution).await?;
    rt.status.stack = spfs::graph::Stack::from_iter(stack);

    // Store additional solve data all the resolved packages as extra
    // data in the spfs runtime so future spk commands run inside the
    // runtime can access it.
    let solve_data = serde_json::to_string(&solution.packages_to_solve_data())
        .map_err(|err| Error::String(err.to_string()))?;
    let spfs_config = spfs::Config::current()?;
    rt.add_annotation(
        SPK_SOLVE_EXTRA_DATA_KEY.to_string(),
        solve_data,
        spfs_config.filesystem.annotation_size_limit,
    )
    .await?;

    rt.save_state_to_storage().await?;
    spfs::remount_runtime(rt).await?;
    Ok(())
}
