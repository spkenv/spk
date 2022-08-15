// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use spk_ident::{parse_ident, PkgRequest, PreReleasePolicy, RangeIdent, RequestedBy};
use spk_solver::{PackageSource, Solution};
use spk_spec_ops::PackageOps;
use spk_storage::{self as storage};

use crate::{Error, Result};

/// Load the current environment from the spfs file system.
pub async fn current_env() -> Result<Solution> {
    match spfs::active_runtime().await {
        Err(spfs::Error::NoActiveRuntime) => {
            return Err(Error::NoEnvironment);
        }
        Err(err) => return Err(err.into()),
        Ok(_) => {}
    }

    let repo = Arc::new(storage::RepositoryHandle::Runtime(Default::default()));
    let mut solution = Solution::new(None);
    for name in repo.list_packages().await? {
        for version in repo.list_package_versions(&name).await?.iter() {
            let pkg = parse_ident(format!("{name}/{version}"))?;
            for pkg in repo.list_package_builds(&pkg).await? {
                let spec = repo.read_package(&pkg).await?;
                let components = match repo.read_components(spec.ident()).await {
                    Ok(c) => c,
                    Err(spk_storage::Error::SpkValidatorsError(
                        spk_validators::Error::PackageNotFoundError(_),
                    )) => {
                        tracing::info!("Skipping missing build {pkg}; currently being built?");
                        continue;
                    }
                    Err(err) => return Err(err.into()),
                };
                let range_ident = RangeIdent::equals(spec.ident(), components.keys().cloned());
                let mut request = PkgRequest::new(range_ident, RequestedBy::CurrentEnvironment);
                request.prerelease_policy = PreReleasePolicy::IncludeAll;
                let repo = repo.clone();
                solution.add(
                    &request,
                    spec,
                    PackageSource::Repository { repo, components },
                );
            }
        }
    }

    Ok(solution)
}
