// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use crate::{api, build, io, solve, storage, Error, Result};
use spfs::encoding::Digest;

/// Pull and list the necessary layers to have all solution packages.
pub fn resolve_runtime_layers(solution: &solve::Solution) -> Result<Vec<Digest>> {
    let local_repo = crate::HANDLE.block_on(storage::local_repository())?;
    let mut stack = Vec::new();
    let mut to_sync = Vec::new();
    for resolved in solution.items() {
        if let solve::PackageSource::Spec(ref source) = resolved.source {
            if source.pkg == resolved.spec.pkg.with_build(None) {
                return Err(Error::String(format!(
                    "Solution includes package that needs building: {}",
                    resolved.spec.pkg
                )));
            }
        }

        let (repo, components) = match resolved.source {
            solve::PackageSource::Repository { repo, components } => (repo, components),
            solve::PackageSource::Spec(_) => continue,
        };

        if resolved.request.pkg.components.is_empty() {
            tracing::warn!(
                "Package request for '{}' identified no components, nothing will be included",
                resolved.request.pkg.name()
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
            .install
            .components
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

            if !crate::HANDLE.block_on(local_repo.has_object(*digest)) {
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
                io::format_ident(&spec.pkg),
            );
            crate::HANDLE.block_on(spfs::sync_ref(digest.to_string(), repo, &local_repo))?;
        }
    }

    Ok(stack)
}

/// Modify the active spfs runtime to include exactly the packages in the given solution.
pub fn setup_current_runtime(solution: &solve::Solution) -> Result<()> {
    let mut rt = spfs::active_runtime()?;
    let stack = resolve_runtime_layers(solution)?;
    rt.reset_stack()?;
    for digest in stack {
        rt.push_digest(&digest)?;
    }
    crate::HANDLE.block_on(spfs::remount_runtime(&rt))?;
    Ok(())
}

/// Build any packages in the given solution that need building.
///
/// Returns a new solution of only binary packages.
pub fn build_required_packages(solution: &solve::Solution) -> Result<solve::Solution> {
    let handle: storage::RepositoryHandle =
        crate::HANDLE.block_on(storage::local_repository())?.into();
    let local_repo = Arc::new(handle);
    let repos = solution.repositories();
    let options = solution.options();
    let mut compiled_solution = solve::Solution::new(Some(options.clone()));
    for item in solution.items() {
        let source_spec = match item.source {
            solve::PackageSource::Spec(spec) if item.is_source_build() => spec,
            source => {
                compiled_solution.add(&item.request, item.spec, source);
                continue;
            }
        };

        tracing::info!(
            "Building: {} for {}",
            io::format_ident(&item.spec.pkg),
            io::format_options(&options)
        );
        let spec = build::BinaryPackageBuilder::from_spec((*source_spec).clone())
            .with_repositories(repos.clone())
            .with_options(options.clone())
            .build()?;
        let components = local_repo.get_package(&spec.pkg)?;
        let source = solve::PackageSource::Repository {
            repo: local_repo.clone(),
            components,
        };
        compiled_solution.add(&item.request, Arc::new(spec), source);
    }
    Ok(compiled_solution)
}

pub mod python {
    use crate::{solve, Digest, Result};
    use pyo3::prelude::*;

    #[pyfunction]
    pub fn resolve_runtime_layers(solution: &solve::Solution) -> Result<Vec<Digest>> {
        Ok(super::resolve_runtime_layers(solution)?
            .into_iter()
            .map(Digest::from)
            .collect())
    }

    #[pyfunction]
    pub fn setup_current_runtime(solution: &solve::Solution) -> Result<()> {
        let _guard = crate::HANDLE.enter();
        super::setup_current_runtime(solution)
    }

    #[pyfunction]
    pub fn build_required_packages(solution: &solve::Solution) -> Result<solve::Solution> {
        super::build_required_packages(solution)
    }

    pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(resolve_runtime_layers, m)?)?;
        m.add_function(wrap_pyfunction!(setup_current_runtime, m)?)?;
        m.add_function(wrap_pyfunction!(build_required_packages, m)?)?;
        Ok(())
    }
}
