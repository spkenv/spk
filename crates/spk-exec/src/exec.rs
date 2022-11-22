// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spfs::encoding::Digest;
use spk_schema::foundation::format::{FormatIdent, FormatOptionMap};
use spk_schema::foundation::ident_component::Component;
use spk_schema::prelude::*;
use spk_solve::solution::{PackageSource, Solution};
use spk_storage::{self as storage};

use crate::{Error, Result};

/// Pull and list the necessary layers to have all solution packages.
pub async fn resolve_runtime_layers(solution: &Solution) -> Result<Vec<Digest>> {
    let local_repo = storage::local_repository().await?;
    let mut stack = Vec::new();
    let mut to_sync = Vec::new();
    for resolved in solution.items() {
        let (repo, components) = match resolved.source {
            PackageSource::Repository { repo, components } => (repo, components),
            PackageSource::Embedded => continue,
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
        };

        if resolved.request.pkg.components.is_empty() {
            tracing::warn!(
                "Package request for '{}' identified no components, nothing will be included",
                resolved.request.pkg.name
            );
            continue;
        }
        let mut desired_components = resolved.request.pkg.components;
        if desired_components.is_empty() {
            desired_components.insert(Component::All);
        }
        if desired_components.remove(&Component::All) {
            desired_components.extend(components.keys().cloned());
        }
        desired_components = resolved
            .spec
            .components()
            .resolve_uses(desired_components.iter());

        for name in desired_components.into_iter() {
            let digest = components.get(&name).ok_or_else(|| {
                Error::String(format!(
                    "Resolved component '{}' went missing, this is likely a bug in the solver",
                    name
                ))
            })?;

            if stack.contains(digest) {
                continue;
            }

            if !local_repo.has_object(*digest).await {
                to_sync.push((resolved.spec.clone(), repo.clone(), *digest))
            }

            stack.push(*digest);
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
