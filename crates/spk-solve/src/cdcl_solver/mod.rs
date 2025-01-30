// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod pkg_request_version_set;
mod spk_provider;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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
        let provider = SpkProvider::new(self.repos.clone());
        let pkg_requirements = provider.pkg_requirements(requests);
        let var_requirements = provider.var_requirements(requests);
        let mut solver = resolvo::Solver::new(provider);
        let problem = resolvo::Problem::new()
            .requirements(pkg_requirements)
            .constraints(var_requirements);
        let solved = solver
            .solve(problem)
            .map_err(|err| Error::String(format!("{err:?}")))?;

        let pool = &solver.provider().pool;
        let mut solution = Solution::default();
        for solvable_id in solved {
            let solvable = pool.resolve_solvable(solvable_id);
            let located_build_ident = &solvable.record;
            let pkg_request = PkgRequest {
                pkg: RangeIdent {
                    repository_name: None,
                    name: located_build_ident.name().to_owned(),
                    components: BTreeSet::new(),
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
                .find(|repo| repo.name() == located_build_ident.repository_name())
                .expect("Expected solved package's repository to be in the list of repositories");
            solution.add(
                pkg_request,
                repo.read_package(located_build_ident.target()).await?,
                PackageSource::Repository {
                    repo: Arc::clone(repo),
                    // XXX: Why is this needed?
                    components: repo.read_components(located_build_ident.target()).await?,
                },
            );
        }
        Ok(solution)
    }
}
