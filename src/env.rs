// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use crate::{
    api, solve,
    storage::{self},
    Error, Result,
};

/// Load the current environment from the spfs file system.
pub fn current_env() -> Result<solve::Solution> {
    match spfs::active_runtime() {
        Err(spfs::Error::NoActiveRuntime) => {
            return Err(Error::NoEnvironment);
        }
        Err(err) => return Err(err.into()),
        Ok(_) => {}
    }

    let repo = Arc::new(storage::RepositoryHandle::Runtime(Default::default()));
    let mut solution = solve::Solution::new(None);
    for name in repo.list_packages()? {
        for version in repo.list_package_versions(&name)? {
            let pkg = api::parse_ident(format!("{name}/{version}"))?;
            for pkg in repo.list_package_builds(&pkg)? {
                let spec = repo.read_spec(&pkg)?;
                let components = repo.get_package(&spec.pkg)?;
                let range_ident = api::RangeIdent::exact(&spec.pkg, components.keys().cloned());
                let mut request = api::PkgRequest::new(range_ident);
                request.prerelease_policy = api::PreReleasePolicy::IncludeAll;
                let repo = repo.clone();
                solution.add(
                    &request,
                    spec.into(),
                    solve::PackageSource::Repository { repo, components },
                );
            }
        }
    }

    Ok(solution)
}
