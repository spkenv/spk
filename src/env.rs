// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use crate::{api, prelude::*, solve, storage, Error, Result};

/// Load the current environment from the spfs file system.
pub async fn current_env() -> Result<solve::Solution> {
    match spfs::active_runtime().await {
        Err(spfs::Error::NoActiveRuntime) => {
            return Err(Error::NoEnvironment);
        }
        Err(err) => return Err(err.into()),
        Ok(_) => {}
    }

    let repo = Arc::new(storage::RepositoryHandle::Runtime(Default::default()));
    let mut solution = solve::Solution::new(None);
    for name in repo.list_packages().await? {
        for version in repo.list_package_versions(&name).await?.iter() {
            let pkg = api::parse_version_ident(format!("{name}/{version}"))?;
            for pkg in repo.list_package_builds(&pkg).await? {
                let spec = repo.read_package(&pkg).await?;
                let components = match repo.read_components(spec.ident()).await {
                    Ok(c) => c,
                    Err(Error::PackageNotFoundError(_)) => {
                        tracing::info!("Skipping missing build {pkg}; currently being built?");
                        continue;
                    }
                    Err(err) => return Err(err),
                };
                let range_ident = api::RangeIdent::equals(
                    spec.ident().clone().into_any(),
                    components.keys().cloned(),
                );
                let mut request =
                    api::PkgRequest::new(range_ident, api::RequestedBy::CurrentEnvironment);
                request.prerelease_policy = api::PreReleasePolicy::IncludeAll;
                let repo = repo.clone();
                solution.add(
                    &request,
                    spec,
                    solve::PackageSource::Repository { repo, components },
                );
            }
        }
    }

    Ok(solution)
}
