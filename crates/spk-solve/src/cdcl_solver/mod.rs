// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

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

use crate::abstract_solver::AbstractSolver;
use crate::{AbstractSolverExt, AbstractSolverMut, DecisionFormatter, Error, Result};

#[cfg(test)]
#[path = "cdcl_solver_tests.rs"]
mod cdcl_solver_tests;

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
                        eprintln!(
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
                            eprintln!("Solver retry {loop_counter}");
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
                match located_build_ident_with_component.ident.build() {
                    spk_schema::ident_build::Build::Source
                        if located_build_ident_with_component.requires_build_from_source =>
                    {
                        PackageSource::BuildFromSource {
                            recipe: repo
                                .read_recipe(
                                    &located_build_ident_with_component.ident.to_version_ident(),
                                )
                                .await?,
                        }
                    }
                    spk_schema::ident_build::Build::Source => {
                        // Not building this from source but just adding the
                        // source build to the Solution.
                        PackageSource::Repository {
                            repo: Arc::clone(repo),
                            // XXX: Why is this needed?
                            components: repo
                                .read_components(located_build_ident_with_component.ident.target())
                                .await?,
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
                                        .read_components(
                                            located_build_ident_with_component.ident.target(),
                                        )
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
                            components: repo
                                .read_components(located_build_ident_with_component.ident.target())
                                .await?,
                        }
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

impl AbstractSolver for Solver {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_options(&self) -> Cow<'_, OptionMap> {
        Cow::Borrowed(&self.options)
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
}

#[async_trait::async_trait]
impl AbstractSolverMut for Solver {
    fn add_request(&mut self, mut request: Request) {
        if let Request::Pkg(request) = &mut request {
            if request.pkg.components.is_empty() {
                if request.pkg.is_source() {
                    request.pkg.components.insert(Component::Source);
                } else {
                    request.pkg.components.insert(Component::default_for_run());
                }
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
impl AbstractSolverExt for Solver {
    fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<RepositoryHandle>>,
    {
        self.repos.push(repo.into());
    }
}
