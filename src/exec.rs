// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::{api, io, solve, storage, Error, Result};
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
        if let storage::RepositoryHandle::SPFS(repo) = &*repo.lock().unwrap() {
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

    pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(resolve_runtime_layers, m)?)?;
        Ok(())
    }
}
