// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod pkg_request_version_set;
mod spk_provider;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pkg_request_version_set::SpkSolvable;
use spk_provider::SpkProvider;
use spk_schema::Request;
use spk_schema::ident::{InclusionPolicy, PinPolicy, PkgRequest, RangeIdent};
use spk_schema::version_range::VersionFilter;
use spk_solve_solution::{PackageSource, Solution};
use spk_solve_validation::Validators;
use spk_storage::RepositoryHandle;

use crate::{Error, Result};

#[cfg(test)]
#[path = "cdcl_solver_tests.rs"]
mod cdcl_solver_tests;

#[derive(Clone)]
pub struct Solver {
    repos: Vec<Arc<RepositoryHandle>>,
    _validators: Cow<'static, [Validators]>,
}

impl Solver {
    pub fn new(repos: Vec<Arc<RepositoryHandle>>, validators: Cow<'static, [Validators]>) -> Self {
        Self {
            repos,
            _validators: validators,
        }
    }

    pub async fn solve(&mut self, requests: &[Request]) -> Result<Solution> {
        let repos = self.repos.clone();
        // XXX: Taking a slice reference doesn't make sense anymore.
        let requests = requests.to_vec();
        // Use a blocking thread so resolvo can call `block_on` on the runtime.
        let solvables = tokio::task::spawn_blocking(move || {
            let mut provider = Some(SpkProvider::new(repos.clone()));
            let (solver, solved) = loop {
                let this_iter_provider = provider.take().expect("provider is always Some");
                let pkg_requirements = this_iter_provider.pkg_requirements(&requests);
                let var_requirements = this_iter_provider.var_requirements(&requests);
                let mut solver = resolvo::Solver::new(this_iter_provider)
                    .with_runtime(tokio::runtime::Handle::current());
                let problem = resolvo::Problem::new()
                    .requirements(pkg_requirements)
                    .constraints(var_requirements);
                match solver.solve(problem) {
                    Ok(solved) => break (solver, solved),
                    Err(resolvo::UnsolvableOrCancelled::Cancelled(_)) => {
                        provider = Some(solver.provider().reset());
                        continue;
                    }
                    Err(resolvo::UnsolvableOrCancelled::Unsolvable(conflict)) => {
                        // Edge case: a need to retry was detected but the
                        // solver arrived at a decision before it noticed it
                        // needs to cancel (unknown if this ever happens).
                        if solver.provider().is_canceled() {
                            provider = Some(solver.provider().reset());
                            continue;
                        }
                        return Err(Error::String(format!(
                            "{}",
                            conflict.display_user_friendly(&solver)
                        )));
                    }
                }
            };

            let pool = &solver.provider().pool;
            Ok(solved
                .into_iter()
                .filter_map(|solvable_id| {
                    let solvable = pool.resolve_solvable(solvable_id);
                    if let SpkSolvable::LocatedBuildIdentWithComponent(
                        located_build_ident_with_component,
                    ) = &solvable.record
                    {
                        Some(located_build_ident_with_component.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>())
        })
        .await
        .map_err(|err| Error::String(format!("Tokio panicked? {err}")))??;

        let mut solution = Solution::default();
        for located_build_ident_with_component in solvables {
            let pkg_request = PkgRequest {
                pkg: RangeIdent {
                    repository_name: None,
                    name: located_build_ident_with_component.ident.name().to_owned(),
                    components: BTreeSet::from_iter([located_build_ident_with_component
                        .component
                        .clone()
                        .into()]),
                    version: VersionFilter::default(),
                    build: None,
                },
                prerelease_policy: None,
                inclusion_policy: InclusionPolicy::default(),
                pin: None,
                pin_policy: PinPolicy::default(),
                required_compat: None,
                requested_by: BTreeMap::new(),
            };
            let repo = self
                .repos
                .iter()
                .find(|repo| {
                    repo.name() == located_build_ident_with_component.ident.repository_name()
                })
                .expect("Expected solved package's repository to be in the list of repositories");
            solution.add(
                pkg_request,
                repo.read_package(located_build_ident_with_component.ident.target())
                    .await?,
                PackageSource::Repository {
                    repo: Arc::clone(repo),
                    // XXX: Why is this needed?
                    components: repo
                        .read_components(located_build_ident_with_component.ident.target())
                        .await?,
                },
            );
        }
        Ok(solution)
    }
}
