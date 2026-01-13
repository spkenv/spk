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
use std::time::Instant;

use pkg_request_version_set::{SpkSolvable, SyntheticComponent};
use spk_provider::SpkProvider;
use spk_schema::ident::{
    InclusionPolicy,
    LocatedBuildIdent,
    PinPolicy,
    PinnedRequest,
    PinnedValue,
    PkgRequest,
    PkgRequestWithOptions,
    RangeIdent,
    RequestWithOptions,
    RequestedBy,
    VarRequest,
};
use spk_schema::ident_component::Component;
use spk_schema::name::PkgNameBuf;
use spk_schema::prelude::{HasVersion, Named, Versioned};
use spk_schema::version_range::VersionFilter;
use spk_schema::{OptionMap, Package, Spec};
use spk_solve_solution::{PackageSource, Solution};
use spk_solve_validation::{Validators, default_validators};
use spk_storage::RepositoryHandle;

use crate::solver::Solver as SolverTrait;
use crate::solvers::resolvo::pkg_request_version_set::LocatedBuildIdentWithComponent;
use crate::{DecisionFormatter, Error, Result, SolverExt, SolverMut, show_search_space_stats};

#[cfg(test)]
#[path = "resolvo_tests.rs"]
mod resolvo_tests;

#[derive(Clone, Default)]
pub struct Solver {
    repos: Vec<Arc<RepositoryHandle>>,
    requests: Vec<RequestWithOptions>,
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

    /// Populate the requested_by field of each PkgRequest in the solution.
    fn populate_requested_by(
        &self,
        solution_adds: Vec<(PkgRequestWithOptions, Arc<Spec>, PackageSource)>,
    ) -> Vec<(PkgRequestWithOptions, Arc<Spec>, PackageSource)> {
        // At this point, almost all the pieces of the solution are in
        // place, but the pkg_requests have the wrong requested by
        // data. This is updated in two passes.
        //
        // First pass: take the runtime requirements (dependency
        // requests) of each resolved package, and get the package
        // name of each requirement, and use that name as key to map
        // to a set that the resolved package's ident is added into.
        // This makes sets of builds that requested each of the
        // package names.
        let mut names_to_requesters: HashMap<PkgNameBuf, BTreeSet<RequestedBy>> = HashMap::new();
        for (_pkg_request, package, _source) in solution_adds.iter() {
            for request in package.runtime_requirements().iter() {
                if let PinnedRequest::Pkg(pkg_req) = request {
                    let name = pkg_req.pkg.name();
                    let entry = names_to_requesters.entry(name.into()).or_default();
                    entry.insert(RequestedBy::PackageBuild(package.ident().clone()));
                }
            }
        }
        // Second pass: go through each package request in the
        // solution and look up its package name in the "name to
        // requesters" mapping, produced above, to get its set of
        // requester. Using the set, update each package request by
        // adding everything in the set to the package request as
        // something that requested it. If no set was found during the
        // look up, and the resolved package isn't embedded, assumes
        // the request was made "from the command line".
        solution_adds
            .into_iter()
            .map(|(mut pkg_request, package, source)| {
                let name = pkg_request.pkg.name();
                if let Some(requesters) = names_to_requesters.get(name) {
                    for requester in requesters {
                        pkg_request.add_requester(requester.clone());
                    }
                    if let PackageSource::Embedded { ref parent, .. } = source {
                        // Embedded case to match the other solver's output
                        pkg_request.add_requester(RequestedBy::Embedded(parent.clone()));
                    }
                } else if let PackageSource::Embedded { ref parent, .. } = source {
                    pkg_request.add_requester(RequestedBy::Embedded(parent.clone()));
                } else {
                    // Try to find the original command line request
                    // based on the solved request's package name.
                    for r in &self.requests {
                        if let RequestWithOptions::Pkg(pkg_req) = r
                            && *pkg_req.pkg.name == *name
                        {
                            for (_, requesters) in pkg_req.requested_by.iter() {
                                for requested_by in requesters {
                                    pkg_request.add_requester(requested_by.clone());
                                }
                            }
                            break;
                        }
                    }
                }
                (pkg_request, package, source)
            })
            .collect()
    }

    pub(crate) fn set_build_from_source_trail(&mut self, trail: HashSet<LocatedBuildIdent>) {
        self.build_from_source_trail = trail;
    }

    pub async fn solve(&self) -> Result<Solution> {
        let repos = self.repos.clone();
        let requests = self.requests.clone();
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
                let var_requirements = this_iter_provider.var_requirements(&requests);
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
                .map(|solvable_id| {
                    let solvable = pool.resolve_solvable(solvable_id);
                    solvable.record.clone()
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
        for spk_solvable in solvables {
            if let SpkSolvable::GlobalVar { key, value } = &spk_solvable {
                solution_options.insert(key.clone(), value.to_string());
                continue;
            }

            let SpkSolvable::LocatedBuildIdentWithComponent(LocatedBuildIdentWithComponent {
                ident,
                component: SyntheticComponent::Actual(solvable_component),
                requires_build_from_source,
            }) = &spk_solvable
            else {
                continue;
            };

            if let Some(existing_index) = seen_packages.get(ident.name()) {
                if let Some((
                    PkgRequestWithOptions {
                        pkg_request:
                            PkgRequest {
                                pkg: RangeIdent { components, .. },
                                ..
                            },
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

            let pkg_request = PkgRequestWithOptions {
                pkg_request: PkgRequest {
                    pkg: RangeIdent {
                        repository_name: None,
                        name: ident.name().to_owned(),
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
                },
                // Does it matter to populate options here?
                options: Default::default(),
            };
            let repo = self
                .repos
                .iter()
                .find(|repo| repo.name() == ident.repository_name())
                .expect("Expected solved package's repository to be in the list of repositories");
            let package = repo.read_package(ident.target()).await?;
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
            for option in package.runtime_requirements().iter() {
                if let PinnedRequest::Var(var_req) = option {
                    solution_options.insert(var_req.var.clone(), var_req.value.to_string());
                }
            }
            let next_index = solution_adds.len();
            seen_packages.insert(ident.name().to_owned(), next_index);
            solution_adds.push((pkg_request, package, {
                match ident.build() {
                    spk_schema::ident_build::Build::Source if *requires_build_from_source => {
                        PackageSource::BuildFromSource {
                            recipe: repo.read_recipe(&ident.clone().to_version_ident()).await?,
                        }
                    }
                    spk_schema::ident_build::Build::Source => {
                        // Not building this from source but just adding the
                        // source build to the Solution.
                        PackageSource::Repository {
                            repo: Arc::clone(repo),
                            // XXX: Why is this needed?
                            components: repo.read_components(ident.target()).await?,
                        }
                    }
                    spk_schema::ident_build::Build::Embedded(embedded_source) => {
                        match embedded_source {
                            spk_schema::ident_build::EmbeddedSource::Package(
                                embedded_source_package,
                            ) => {
                                PackageSource::Embedded {
                                    parent: (**embedded_source_package).clone().try_into()?,
                                    // XXX: Why is this needed?
                                    components: repo
                                        .read_components(ident.target())
                                        .await?
                                        .keys()
                                        .cloned()
                                        .collect(),
                                }
                            }
                            spk_schema::ident_build::EmbeddedSource::Unknown => todo!(),
                        }
                    }
                    spk_schema::ident_build::Build::BuildId(_build_id) => {
                        PackageSource::Repository {
                            repo: Arc::clone(repo),
                            // XXX: Why is this needed?
                            components: repo.read_components(ident.target()).await?,
                        }
                    }
                }
            }));
        }

        let solution_adds = self.populate_requested_by(solution_adds);

        let mut solution = Solution::new(solution_options);
        for (pkg_request, package, source) in solution_adds {
            solution.add(pkg_request, package, source);
        }
        Ok(solution)
    }
}

impl SolverTrait for Solver {
    fn get_options(&self) -> Cow<'_, OptionMap> {
        Cow::Borrowed(&self.options)
    }

    fn get_pkg_requests(&self) -> Vec<PkgRequestWithOptions> {
        self.requests
            .iter()
            .filter_map(|r| r.pkg_ref())
            .cloned()
            .collect()
    }

    fn get_var_requests(&self) -> Vec<VarRequest<PinnedValue>> {
        self.requests
            .iter()
            .filter_map(|r| r.var_ref())
            .cloned()
            .collect()
    }

    fn repositories(&self) -> &[Arc<RepositoryHandle>] {
        &self.repos
    }
}

#[async_trait::async_trait]
impl SolverMut for Solver {
    fn add_request(&mut self, mut request: RequestWithOptions) {
        if let RequestWithOptions::Pkg(request) = &mut request
            && request.pkg.components.is_empty()
        {
            if request.pkg.is_source() {
                request.pkg.components.insert(Component::Source);
            } else {
                request.pkg.components.insert(Component::default_for_run());
            }
        }
        self.requests.push(request);
    }

    fn reset(&mut self) {
        self.repos.truncate(0);
        self.requests.truncate(0);
        self._validators = Cow::from(default_validators());
    }

    async fn run_and_log_resolve(&mut self, formatter: &DecisionFormatter) -> Result<Solution> {
        // This solver doesn't currently support tracing.
        self.run_and_print_resolve(formatter).await
    }

    async fn run_and_print_resolve(&mut self, formatter: &DecisionFormatter) -> Result<Solution> {
        let solution = self.solve().await?;
        let output = solution
            .format_solution_with_highest_versions(
                formatter.settings.verbosity,
                self.repositories(),
                // the order coming out of resolvo is ... random?
                true,
            )
            .await?;
        if formatter.settings.show_solution {
            println!("{output}");
        }

        // Equivalent to DecisionFormatter's show_search_space_info() for the StepSolver
        if formatter.settings.show_search_space_size {
            tracing::info!("Calculating search space stats. This may take some time...");
            let start = Instant::now();

            let initial_requests = self
                .get_pkg_requests()
                .iter()
                .map(|r| r.pkg.to_string())
                .collect::<Vec<String>>();

            show_search_space_stats(
                &initial_requests,
                &solution,
                &self.repos,
                formatter.settings.verbosity,
            )
            .await?;
            tracing::info!("That took {} seconds", start.elapsed().as_secs_f64());
        }

        Ok(solution)
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

#[async_trait::async_trait]
impl SolverExt for Solver {
    fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<RepositoryHandle>>,
    {
        self.repos.push(repo.into());
    }
}
