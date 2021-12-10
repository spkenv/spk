// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::{solve, storage, Error, Result};
use spfs::encoding::Digest;

/// Pull and list the necessary layers to have all solution packages.
pub fn resolve_runtime_layers(solution: &solve::Solution) -> Result<Vec<Digest>> {
    let mut local_repo = storage::local_repository()?;
    let mut stack = Vec::new();
    let mut to_sync = Vec::new();
    for resolved in solution.items() {
        if let solve::PackageSource::Spec(ref source) = resolved.source {
            if source.pkg == resolved.spec.pkg.with_build(None) {
                return Err(Error::String(format!(
                    "Solution includes package that needs building: {}",
                    spec.pkg
                )));
            }
        }

        let repo = match resolved.source {
            solve::PackageSource::Repository(repo) => repo,
            solve::PackageSource::Spec(_) => continue,
        };

        let digest = repo
            .lock()
            .unwrap()
            .get_package(&resolved.spec.pkg)
            .map_err(|err| match err {
                Error::PackageNotFoundError(pkg) => Error::String(format!(
                    "Resolved package disappeared, please try again ({})",
                    pkg
                )),
                _ => err,
            })?;

        if !local_repo.has_object(&digest) {
            to_sync.push((resolved.spec, repo.clone(), digest))
        }

        stack.push(digest);
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
            spfs::sync_ref(digest.to_string(), repo, &mut local_repo)?;
        }
    }

    Ok(stack)
}
