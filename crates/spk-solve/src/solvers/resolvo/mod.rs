// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! A CDCL SAT solver for Spk.
//!
//! This solver uses [Resolvo](https://github.com/prefix-dev/resolvo) and is
//! able to handle more complex problems than the original Spk solver. However
//! the tradeoff is that it requires reading all the package metadata up front
//! so it can be slower than the original solver for small cases.
//!
//! When there is no solution, Resolvo provides a useful error message to help
//! explain the problem whereas the original solver requires reading the solver
//! log to deduce the real cause of the failure.

mod pkg_request_version_set;
mod spk_provider;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use pkg_request_version_set::{SpkSolvable, SyntheticComponent};
use spk_provider::SpkProvider;
use spk_schema::ident::{
    InclusionPolicy,
    LocatedBuildIdent,
    PinPolicy,
    PkgRequest,
    RangeIdent,
    VarRequest,
};
use spk_schema::ident_component::Component;
use spk_schema::prelude::{HasVersion, Named, Versioned};
use spk_schema::version_range::VersionFilter;
use spk_schema::{OptionMap, Package, Request};
use spk_solve_solution::{PackageSource, Solution};
use spk_solve_validation::{Validators, default_validators};
use spk_storage::RepositoryHandle;

use crate::solver::Solver as SolverTrait;
use crate::{Error, Result};

#[cfg(test)]
#[path = "resolvo_tests.rs"]
mod resolvo_tests;

#[derive(Clone, Default)]
pub struct Solver {
    repos: Vec<Arc<RepositoryHandle>>,
    requests: Vec<Request>,
    options: OptionMap,
    binary_only: bool,
    _validators: Cow<'static, [Validators]>,
    build_from_source_trail: HashSet<LocatedBuildIdent>,
}

impl Solver {
    pub fn new(repos: Vec<Arc<RepositoryHandle>>, validators: Cow<'static, [Validators]>) -> Self {
        Self {
            repos,
            requests: Vec::new(),
            options: Default::default(),
            binary_only: true,
            _validators: validators,
            build_from_source_trail: HashSet::new(),
        }
    }

    pub(crate) fn set_build_from_source_trail(&mut self, trail: HashSet<LocatedBuildIdent>) {
        self.build_from_source_trail = trail;
    }

    pub async fn solve(&self) -> Result<Solution> {
        let repos = self.repos.clone();
        let requests = self.requests.clone();
        let options = self.options.clone();
        let binary_only = self.binary_only;
        let build_from_source_trail = self.build_from_source_trail.clone();
        // Use a blocking thread so resolvo can call `block_on` on the runtime.
        let solvables = tokio::task::spawn_blocking(move || {
            let mut provider = Some(SpkProvider::new(
                repos.clone(),
                binary_only,
                build_from_source_trail,
            ));
            let mut loop_counter = 0;
            let (solver, solved) = loop {
                loop_counter += 1;
                let mut this_iter_provider = provider.take().expect("provider is always Some");
                let pkg_requirements = this_iter_provider.root_pkg_requirements(&requests);
                let mut var_requirements = this_iter_provider.var_requirements(&requests);
                // XXX: Not sure if this will result in the desired precedence
                // when options and var requests for the same thing exist.
                var_requirements
                    .extend(this_iter_provider.var_requirements_from_options(options.clone()));
                let mut solver = resolvo::Solver::new(this_iter_provider)
                    .with_runtime(tokio::runtime::Handle::current());
                let problem = resolvo::Problem::new()
                    .requirements(pkg_requirements)
                    .constraints(var_requirements);
                match solver.solve(problem) {
                    Ok(solved) => break (solver, solved),
                    Err(resolvo::UnsolvableOrCancelled::Cancelled(msg)) => {
                        let msg = msg.downcast_ref::<String>();
                        provider = Some(solver.provider().reset());
                        tracing::info!(
                            "Solver retry {loop_counter}: {msg:?}",
                            msg = msg.map_or("unknown", |v| v)
                        );
                        continue;
                    }
                    Err(resolvo::UnsolvableOrCancelled::Unsolvable(conflict)) => {
                        // Edge case: a need to retry was detected but the
                        // solver arrived at a decision before it noticed it
                        // needs to cancel (unknown if this ever happens).
                        if solver.provider().is_canceled() {
                            provider = Some(solver.provider().reset());
                            tracing::info!("Solver retry {loop_counter}");
                            continue;
                        }
                        return Err(Error::FailedToResolve(format!(
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

        let mut solution_options = OptionMap::default();
        let mut solution_adds = Vec::with_capacity(solvables.len());
        // Keep track of the index of each package added to `solution_adds` in
        // order to merge components. Components of a package come out of the
        // solver as separate solvables. The solver logic guarantees that any
        // two entries in this list with the same package name are for the same
        // package and this merging of components is valid.
        let mut seen_packages = HashMap::new();
        for located_build_ident_with_component in solvables {
            let SyntheticComponent::Actual(solvable_component) =
                &located_build_ident_with_component.component
            else {
                continue;
            };

            if let Some(existing_index) =
                seen_packages.get(located_build_ident_with_component.ident.name())
            {
                if let Some((
                    PkgRequest {
                        pkg: RangeIdent { components, .. },
                        ..
                    },
                    _,
                    _,
                )) = solution_adds.get_mut(*existing_index)
                {
                    // If we visit a solvable for the "All" component, the
                    // solver guarantees that we will have all the components.
                    if !components.contains(&Component::All) {
                        components.insert(solvable_component.clone());
                    } else if solvable_component.is_all() {
                        *components = BTreeSet::from([Component::All]);
                    }
                }
                continue;
            }

            let pkg_request = PkgRequest {
                pkg: RangeIdent {
                    repository_name: None,
                    name: located_build_ident_with_component.ident.name().to_owned(),
                    components: BTreeSet::from_iter([solvable_component.clone()]),
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
            let package = repo
                .read_package(located_build_ident_with_component.ident.target())
                .await?;
            let rendered_version = package.compat().render(package.version());
            solution_options.insert(package.name().as_opt_name().to_owned(), rendered_version);
            for option in package.get_build_options() {
                match option {
                    spk_schema::Opt::Pkg(pkg_opt) => {
                        if let Some(value) = pkg_opt.get_value(None) {
                            solution_options.insert(
                                format!("{}.{}", package.name(), pkg_opt.pkg).try_into().expect("Two packages names separated by a period is a valid option name"),
                                value,
                            );
                        }
                    }
                    spk_schema::Opt::Var(var_opt) => {
                        if let Some(value) = var_opt.get_value(None) {
                            if var_opt.var.namespace().is_none() {
                                solution_options.insert(
                                    format!("{}.{}", package.name(), var_opt.var).try_into().expect("A package name, a period, and a non-namespaced option name is a valid option name"),
                                    value,
                                );
                            } else {
                                solution_options.insert(var_opt.var.clone(), value);
                            }
                        }
                    }
                }
            }
            let next_index = solution_adds.len();
            seen_packages.insert(
                located_build_ident_with_component.ident.name().to_owned(),
                next_index,
            );
            solution_adds.push((pkg_request, package, {
                if located_build_ident_with_component.requires_build_from_source {
                    PackageSource::BuildFromSource {
                        recipe: repo
                            .read_recipe(
                                &located_build_ident_with_component.ident.to_version_ident(),
                            )
                            .await?,
                    }
                } else {
                    PackageSource::Repository {
                        repo: Arc::clone(repo),
                        // XXX: Why is this needed?
                        components: repo
                            .read_components(located_build_ident_with_component.ident.target())
                            .await?,
                    }
                }
            }));
        }
        let mut solution = Solution::new(solution_options);
        for (pkg_request, package, source) in solution_adds {
            solution.add(pkg_request, package, source);
        }
        Ok(solution)
    }
}

#[async_trait::async_trait]
impl SolverTrait for Solver {
    fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<RepositoryHandle>>,
    {
        self.repos.push(repo.into());
    }

    fn add_request(&mut self, request: Request) {
        self.requests.push(request);
    }

    fn get_pkg_requests(&self) -> Vec<PkgRequest> {
        self.requests
            .iter()
            .filter_map(|r| r.pkg_ref())
            .cloned()
            .collect()
    }

    fn get_var_requests(&self) -> Vec<VarRequest> {
        self.requests
            .iter()
            .filter_map(|r| r.var_ref())
            .cloned()
            .collect()
    }

    fn repositories(&self) -> &[Arc<RepositoryHandle>] {
        &self.repos
    }

    fn reset(&mut self) {
        self.repos.truncate(0);
        self.requests.truncate(0);
        self._validators = Cow::from(default_validators());
    }

    fn set_binary_only(&mut self, binary_only: bool) {
        self.binary_only = binary_only;
    }

    async fn solve(&mut self) -> Result<Solution> {
        Solver::solve(self).await
    }

    fn update_options(&mut self, options: OptionMap) {
        self.options.extend(options);
    }
}
