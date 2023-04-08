// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::sync::Arc;

use async_stream::try_stream;
use futures::Stream;
use relative_path::RelativePathBuf;
use spfs::encoding::Digest;
use spfs::graph::Object;
use spfs::tracking::Entry;
use spk_schema::foundation::format::{FormatIdent, FormatOptionMap};
use spk_schema::foundation::ident_component::Component;
use spk_schema::prelude::*;
use spk_schema::Spec;
use spk_solve::solution::{PackageSource, Solution};
use spk_solve::RepositoryHandle;
use spk_storage::{self as storage};

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
        try_stream! {
            for resolved_layer in self.0.iter() {
                let manifest = match &*resolved_layer.repo {
                    RepositoryHandle::SPFS(repo) => {
                        let object = repo.read_object(resolved_layer.digest).await?;
                        match object {
                            Object::Layer(obj) => {
                                match repo.read_object(obj.manifest).await? {
                                    Object::Manifest(obj) => obj,
                                    _ => continue,
                                }
                            }
                            Object::Manifest(obj) => obj,
                            _ => continue,
                        }
                    }
                    _ => Err(Error::NonSPFSLayerInResolvedLayers)?,
                };
                let unlock = manifest.to_tracking_manifest();
                let walker = unlock.walk();
                for node in walker {
                    yield (node.path, node.entry.clone(), resolved_layer)
                }
            }
        }
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

/// Pull and list the necessary layers to have all solution packages.
pub async fn resolve_runtime_layers(solution: &Solution) -> Result<Vec<Digest>> {
    pull_resolved_runtime_layers(&solution_to_resolved_runtime_layers(solution)?).await
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
    let stack = resolve_runtime_layers(solution).await?;
    rt.status.stack = stack;
    rt.save_state_to_storage().await?;
    spfs::remount_runtime(rt).await?;
    Ok(())
}
