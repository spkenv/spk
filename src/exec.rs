// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use crate::{
    api, build,
    io::{self, Format},
    prelude::*,
    solve, storage, Error, Result,
};
use spfs::encoding::Digest;

/// Pull and list the necessary layers to have all solution packages.
pub async fn resolve_runtime_layers(solution: &solve::Solution) -> Result<Vec<Digest>> {
    let local_repo = storage::local_repository().await?;
    let mut stack = Vec::new();
    let mut to_sync = Vec::new();
    for resolved in solution.items() {
        let (repo, components) = match resolved.source {
            solve::PackageSource::Repository { repo, components } => (repo, components),
            solve::PackageSource::Embedded => continue,
            solve::PackageSource::BuildFromSource { recipe } => {
                // The resolved solution includes a package that needs
                // to be built with specific options because such a
                // build doesn't exist in a repo.
                let spec_options = recipe
                    .resolve_options(&solution.options())
                    .unwrap_or_default();
                return Err(Error::String(format!(
                    "Solution includes package that needs building from source: {} with these options: {}",
                    resolved.spec.ident(),
                    io::format_options(&spec_options)
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
            desired_components.insert(api::Component::All);
        }
        if desired_components.remove(&api::Component::All) {
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
pub async fn setup_current_runtime(solution: &solve::Solution) -> Result<()> {
    let mut rt = spfs::active_runtime().await?;
    setup_runtime(&mut rt, solution).await
}

pub async fn setup_runtime(
    rt: &mut spfs::runtime::Runtime,
    solution: &solve::Solution,
) -> Result<()> {
    let stack = resolve_runtime_layers(solution).await?;
    rt.status.stack = stack;
    rt.save_state_to_storage().await?;
    spfs::remount_runtime(rt).await?;
    Ok(())
}

/// Build any packages in the given solution that need building.
///
/// Returns a new solution of only binary packages.
pub async fn build_required_packages(solution: &solve::Solution) -> Result<solve::Solution> {
    let handle: storage::RepositoryHandle = storage::local_repository().await?.into();
    let local_repo = Arc::new(handle);
    let repos = solution.repositories();
    let options = solution.options();
    let mut compiled_solution = solve::Solution::new(Some(options.clone()));
    for item in solution.items() {
        let recipe = match item.source {
            solve::PackageSource::BuildFromSource { recipe } => recipe,
            source => {
                compiled_solution.add(&item.request, item.spec, source);
                continue;
            }
        };

        tracing::info!(
            "Building: {} for {}",
            item.spec.ident().format_ident(),
            io::format_options(&options)
        );
        let (package, components) = build::BinaryPackageBuilder::from_recipe((*recipe).clone())
            .with_repositories(repos.clone())
            .with_options(options.clone())
            .build_and_publish(&*local_repo)
            .await?;
        let source = solve::PackageSource::Repository {
            repo: local_repo.clone(),
            components,
        };
        compiled_solution.add(&item.request, Arc::new(package), source);
    }
    Ok(compiled_solution)
}
