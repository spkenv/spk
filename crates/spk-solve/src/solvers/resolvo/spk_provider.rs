// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use resolvo::utils::Pool;
use resolvo::{
    Candidates,
    Dependencies,
    DependencyProvider,
    Interner,
    KnownDependencies,
    NameId,
    Requirement,
    SolvableId,
    SolverCache,
    StringId,
    VersionSetId,
    VersionSetUnionId,
};
use spk_schema::foundation::pkg_name;
use spk_schema::ident::{
    LocatedBuildIdent,
    PinnableValue,
    PkgRequest,
    RangeIdent,
    RequestedBy,
    Satisfy,
    VarRequest,
};
use spk_schema::ident_build::{Build, EmbeddedSource, EmbeddedSourcePackage};
use spk_schema::ident_component::Component;
use spk_schema::name::{OptNameBuf, PkgNameBuf};
use spk_schema::prelude::{HasVersion, Named};
use spk_schema::version::Version;
use spk_schema::version_range::{DoubleEqualsVersion, Ranged, VersionFilter, parse_version_range};
use spk_schema::{BuildIdent, Deprecate, Opt, Package, Recipe, Request, VersionIdent};
use spk_storage::RepositoryHandle;
use tracing::{Instrument, debug_span};

use super::pkg_request_version_set::{
    LocatedBuildIdentWithComponent,
    RequestVS,
    SpkSolvable,
    SyntheticComponent,
    VarValue,
};
use crate::Solver;

// "global-vars--" represents a pseudo- package that accumulates global var
// constraints. The name is intended to never conflict with a real package name,
// however since it is a legal package name that can't be guaranteed.
const PSEUDO_PKG_NAME_PREFIX: &str = "global-vars--";

// Using just the package name as a Resolvo "package name" prevents multiple
// components from the same package from existing in the same solution, since
// we consider the different components to be different "solvables". Instead,
// treat different components of a package as separate packages. There needs to
// be a relationship between every component of a package and a "base"
// component, to prevent a solve containing a mix of components from different
// versions of the same package.
#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct PkgNameBufWithComponent {
    pub(crate) name: PkgNameBuf,
    pub(crate) component: SyntheticComponent,
}

impl std::fmt::Display for PkgNameBufWithComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.component {
            SyntheticComponent::Base => write!(f, "{}", self.name),
            SyntheticComponent::Actual(component) => write!(f, "{}:{component}", self.name),
        }
    }
}

enum CanBuildFromSource {
    Yes,
    No(StringId),
}

pub(crate) struct SpkProvider {
    pub(crate) pool: Pool<RequestVS, PkgNameBufWithComponent>,
    repos: Vec<Arc<RepositoryHandle>>,
    /// Global options, like what might be specified with `--opt` to `spk env`.
    /// Indexed by name. If multiple requests happen to exist with the same
    /// name, the last one is kept.
    global_var_requests: HashMap<OptNameBuf, VarRequest<PinnableValue>>,
    interned_solvables: RefCell<HashMap<SpkSolvable, SolvableId>>,
    /// Track all the global var keys and values that have been witnessed while
    /// solving.
    known_global_var_values: RefCell<HashMap<String, HashSet<VarValue>>>,
    /// Track which global var candidates have been queried by the solver. Once
    /// queried, it is no longer possible to add more possible values without
    /// restarting the solve.
    queried_global_var_values: RefCell<HashSet<String>>,
    cancel_solving: Cell<bool>,
    binary_only: bool,
    /// When recursively exploring building packages from source, track chain
    /// of packages to detect cycles.
    build_from_source_trail: RefCell<HashSet<LocatedBuildIdent>>,
}

impl SpkProvider {
    /// Return if the given solvable is buildable from source considering the
    /// existing requests.
    async fn can_build_from_source(&self, ident: &LocatedBuildIdent) -> CanBuildFromSource {
        if self.build_from_source_trail.borrow().contains(ident) {
            return CanBuildFromSource::No(
                self.pool
                    .intern_string(format!("cycle detected while building {ident} from source")),
            );
        }

        // Get solver requirements from the recipe.
        let recipe = match self
            .repos
            .iter()
            .find(|repo| repo.name() == ident.repository_name())
        {
            Some(repo) => match repo.read_recipe(&ident.clone().to_version_ident()).await {
                Ok(recipe) if recipe.is_deprecated() => {
                    return CanBuildFromSource::No(
                        self.pool
                            .intern_string(format!("recipe for {ident} is deprecated")),
                    );
                }
                Ok(recipe) => recipe,
                Err(err) => {
                    return CanBuildFromSource::No(
                        self.pool
                            .intern_string(format!("failed to read recipe: {err}")),
                    );
                }
            },
            None => {
                return CanBuildFromSource::No(
                    self.pool
                        .intern_string("package's repository is not in the list of repositories"),
                );
            }
        };

        // Do we try all the variants in the recipe?
        let variants = recipe.default_variants(
            // XXX: What should really go here?
            &Default::default(),
        );

        let mut solve_errors = Vec::new();

        for variant in variants.iter() {
            let mut solver = super::Solver::new(self.repos.clone(), Cow::Borrowed(&[]));
            solver.set_binary_only(false);
            solver.set_build_from_source_trail(HashSet::from_iter(
                self.build_from_source_trail
                    .borrow()
                    .iter()
                    .cloned()
                    .chain([ident.clone()]),
            ));

            let build_requirements = match recipe.get_build_requirements(&variant) {
                Ok(build_requirements) => build_requirements,
                Err(err) => {
                    return CanBuildFromSource::No(
                        self.pool
                            .intern_string(format!("failed to get build requirements: {err}")),
                    );
                }
            };

            for request in build_requirements.iter() {
                solver.add_request(request.clone());
            }

            // These are last to take priority over the requests in the recipe.
            for request in self.global_var_requests.values() {
                solver.add_request(Request::Var(request.clone()));
            }

            match solver
                .solve()
                .instrument(debug_span!(
                    "recursive solve",
                    ident = ident.to_string(),
                    variant = variant.to_string()
                ))
                .await
            {
                Ok(_solution) => return CanBuildFromSource::Yes,
                Err(err) => solve_errors.push(err),
            };
        }

        CanBuildFromSource::No(
            self.pool
                .intern_string(format!("failed to build from source: {solve_errors:?}")),
        )
    }

    pub fn new(
        repos: Vec<Arc<RepositoryHandle>>,
        binary_only: bool,
        build_from_source_trail: HashSet<LocatedBuildIdent>,
    ) -> Self {
        Self {
            pool: Pool::new(),
            repos,
            global_var_requests: Default::default(),
            interned_solvables: Default::default(),
            known_global_var_values: Default::default(),
            queried_global_var_values: Default::default(),
            cancel_solving: Default::default(),
            binary_only,
            build_from_source_trail: RefCell::new(build_from_source_trail),
        }
    }

    fn pkg_request_to_known_dependencies(&self, pkg_request: &PkgRequest) -> KnownDependencies {
        let mut components = pkg_request.pkg.components.iter().peekable();
        let iter = if components.peek().is_some() {
            itertools::Either::Right(components.cloned().map(SyntheticComponent::Actual))
        } else {
            itertools::Either::Left(
                // A request with no components is assumed to be a request for
                // the run (or source) component.
                if pkg_request
                    .pkg
                    .build
                    .as_ref()
                    .map(|build| build.is_source())
                    .unwrap_or(false)
                {
                    std::iter::once(SyntheticComponent::Actual(Component::Source))
                } else {
                    std::iter::once(SyntheticComponent::Actual(Component::Run))
                },
            )
        };
        let mut known_deps = KnownDependencies {
            requirements: Vec::new(),
            constrains: Vec::new(),
        };
        for component in iter {
            let dep_name = self.pool.intern_package_name(PkgNameBufWithComponent {
                name: pkg_request.pkg.name().to_owned(),
                component,
            });
            let dep_vs = self.pool.intern_version_set(
                dep_name,
                RequestVS::SpkRequest(Request::Pkg(pkg_request.clone())),
            );
            match pkg_request.inclusion_policy {
                spk_schema::ident::InclusionPolicy::Always => {
                    known_deps.requirements.push(dep_vs.into());
                }
                spk_schema::ident::InclusionPolicy::IfAlreadyPresent => {
                    known_deps.constrains.push(dep_vs);
                }
            }
        }
        known_deps
    }

    pub fn pkg_requirements(&self, requests: &[Request]) -> Vec<Requirement> {
        requests
            .iter()
            .filter_map(|req| match req {
                Request::Pkg(pkg) => Some(pkg),
                _ => None,
            })
            .flat_map(|req| self.pkg_request_to_known_dependencies(req).requirements)
            .collect()
    }

    pub fn is_canceled(&self) -> bool {
        self.cancel_solving.get()
    }

    fn request_to_known_dependencies(&self, requirement: &Request) -> KnownDependencies {
        let mut known_deps = KnownDependencies::default();
        match requirement {
            Request::Pkg(pkg_request) => {
                let kd = self.pkg_request_to_known_dependencies(pkg_request);
                known_deps.requirements.extend(kd.requirements);
                known_deps.constrains.extend(kd.constrains);
            }
            Request::Var(var_request) => {
                match &var_request.value {
                    spk_schema::ident::PinnableValue::FromBuildEnv => todo!(),
                    spk_schema::ident::PinnableValue::FromBuildEnvIfPresent => todo!(),
                    spk_schema::ident::PinnableValue::Pinned(value) => {
                        let dep_name = match var_request.var.namespace() {
                            Some(pkg_name) => {
                                self.pool.intern_package_name(PkgNameBufWithComponent {
                                    name: pkg_name.to_owned(),
                                    component: SyntheticComponent::Base,
                                })
                            }
                            None => {
                                // Since we will be adding constraints for
                                // global vars we need to add the pseudo-package
                                // to the dependency list so it will influence
                                // decisions.
                                let pseudo_pkg_name = format!(
                                    "{PSEUDO_PKG_NAME_PREFIX}{}",
                                    var_request.var.base_name()
                                );
                                if self
                                    .known_global_var_values
                                    .borrow_mut()
                                    .entry(var_request.var.base_name().to_owned())
                                    .or_default()
                                    .insert(VarValue::ArcStr(Arc::clone(value)))
                                    && self
                                        .queried_global_var_values
                                        .borrow()
                                        .contains(var_request.var.base_name())
                                {
                                    // Seeing a new value for a var that has
                                    // already locked in the list of candidates.
                                    self.cancel_solving.set(true);
                                }
                                let dep_name =
                                    self.pool.intern_package_name(PkgNameBufWithComponent {
                                        name: pkg_name!(&pseudo_pkg_name).to_owned(),
                                        component: SyntheticComponent::Base,
                                    });
                                known_deps.requirements.push(
                                    self.pool
                                        .intern_version_set(
                                            dep_name,
                                            RequestVS::GlobalVar {
                                                key: var_request.var.base_name().to_owned(),
                                                value: VarValue::ArcStr(Arc::clone(value)),
                                            },
                                        )
                                        .into(),
                                );
                                dep_name
                            }
                        };
                        // If we end up adding pkg_name to the solve, it needs
                        // to satisfy this var request.
                        known_deps.constrains.push(self.pool.intern_version_set(
                            dep_name,
                            RequestVS::SpkRequest(requirement.clone()),
                        ));
                    }
                }
            }
        }
        known_deps
    }

    /// Return a new provider to restart the solve, preserving what was learned
    /// about global variables.
    pub fn reset(&self) -> Self {
        Self {
            pool: Pool::new(),
            repos: self.repos.clone(),
            global_var_requests: self.global_var_requests.clone(),
            interned_solvables: Default::default(),
            known_global_var_values: RefCell::new(self.known_global_var_values.take()),
            queried_global_var_values: Default::default(),
            cancel_solving: Default::default(),
            binary_only: self.binary_only,
            build_from_source_trail: self.build_from_source_trail.clone(),
        }
    }

    pub fn var_requirements(&mut self, requests: &[Request]) -> Vec<VersionSetId> {
        self.global_var_requests.reserve(requests.len());
        requests
            .iter()
            .filter_map(|req| match req {
                Request::Var(var) => Some(var),
                _ => None,
            })
            .filter_map(|req| match req.var.namespace() {
                Some(pkg_name) => {
                    // A global request applicable to a specific package.
                    let dep_name = self.pool.intern_package_name(PkgNameBufWithComponent {
                        name: pkg_name.to_owned(),
                        component: SyntheticComponent::Base,
                    });
                    Some(self.pool.intern_version_set(
                        dep_name,
                        RequestVS::SpkRequest(Request::Var(req.clone())),
                    ))
                }
                None => {
                    // A global request affecting all packages.
                    self.global_var_requests
                        .insert(req.var.without_namespace().to_owned(), req.clone());
                    None
                }
            })
            .collect()
    }
}

impl DependencyProvider for SpkProvider {
    async fn filter_candidates(
        &self,
        candidates: &[SolvableId],
        version_set: VersionSetId,
        inverse: bool,
    ) -> Vec<SolvableId> {
        let mut selected = Vec::with_capacity(candidates.len());
        let request_vs = self.pool.resolve_version_set(version_set);
        for candidate in candidates {
            let solvable = self.pool.resolve_solvable(*candidate);
            match &request_vs {
                RequestVS::SpkRequest(Request::Pkg(pkg_request)) => {
                    let SpkSolvable::LocatedBuildIdentWithComponent(
                        located_build_ident_with_component,
                    ) = &solvable.record
                    else {
                        if inverse {
                            selected.push(*candidate);
                        }
                        continue;
                    };

                    let compatible = pkg_request
                        .is_version_applicable(located_build_ident_with_component.ident.version());
                    if compatible.is_ok() {
                        let is_source =
                            located_build_ident_with_component.ident.build().is_source();

                        // If build from source is enabled, any source build is
                        // a candidate. Source builds that can't be built from
                        // source are filtered out in `get_candidates`.
                        if located_build_ident_with_component.requires_build_from_source {
                            if !inverse {
                                selected.push(*candidate);
                            }
                            continue;
                        }

                        // Only select source builds for requests of source builds.
                        if is_source {
                            if pkg_request
                                .pkg
                                .build
                                .as_ref()
                                .map(|build| build.is_source())
                                .unwrap_or(false)
                                ^ inverse
                            {
                                selected.push(*candidate);
                            }
                            continue;
                        }

                        // Only select All component for requests of All
                        // component.
                        if located_build_ident_with_component.component.is_all() {
                            if pkg_request.pkg.components.contains(&Component::All) ^ inverse {
                                selected.push(*candidate);
                            }
                            continue;
                        } else
                        // Only the All component can satisfy requests for All.
                        if pkg_request.pkg.components.contains(&Component::All) {
                            if inverse {
                                selected.push(*candidate);
                            }
                            continue;
                        }

                        // Only the x component can satisfy requests for x.
                        let mut at_least_one_request_matched_this_solvable = None;
                        for component in pkg_request.pkg.components.iter() {
                            if component.is_all() {
                                continue;
                            }
                            if component == &located_build_ident_with_component.component {
                                at_least_one_request_matched_this_solvable = Some(true);
                                break;
                            } else {
                                at_least_one_request_matched_this_solvable = Some(false);
                            }
                        }

                        match at_least_one_request_matched_this_solvable {
                            Some(true) => {
                                if inverse {
                                    continue;
                                }
                            }
                            Some(false) => {
                                // The request is for specific components but
                                // this solvable doesn't match any of them.
                                if inverse {
                                    selected.push(*candidate);
                                    continue;
                                }
                            }
                            None => {
                                // TODO: if at_least_one_request_matched_this_solvable
                                // is None it means the request didn't specify a
                                // component. Decide which specific component this
                                // should match.
                            }
                        }

                        // XXX: This find runtime will add up.
                        let repo = self
                        .repos
                        .iter()
                        .find(|repo| repo.name() == located_build_ident_with_component.ident.repository_name())
                        .expect(
                            "Expected solved package's repository to be in the list of repositories",
                        );
                        if let Ok(package) = repo
                            .read_package(located_build_ident_with_component.ident.target())
                            .await
                        {
                            if pkg_request.is_satisfied_by(&package).is_ok() ^ inverse {
                                selected.push(*candidate);
                            }
                        } else if inverse {
                            // If reading the package failed but inverse is true, should
                            // we include the package as a candidate? Unclear.
                            selected.push(*candidate);
                        }
                    } else if inverse {
                        selected.push(*candidate);
                    }
                }
                RequestVS::SpkRequest(Request::Var(var_request)) => {
                    match var_request.var.namespace() {
                        Some(pkg_name) => {
                            let SpkSolvable::LocatedBuildIdentWithComponent(
                                located_build_ident_with_component,
                            ) = &solvable.record
                            else {
                                if inverse {
                                    selected.push(*candidate);
                                }
                                continue;
                            };
                            // Will this ever not match?
                            debug_assert_eq!(
                                pkg_name,
                                located_build_ident_with_component.ident.name()
                            );
                            // XXX: This find runtime will add up.
                            let repo = self
                        .repos
                        .iter()
                        .find(|repo| repo.name() == located_build_ident_with_component.ident.repository_name())
                        .expect(
                            "Expected solved package's repository to be in the list of repositories",
                        );
                            if let Ok(package) = repo
                                .read_package(located_build_ident_with_component.ident.target())
                                .await
                            {
                                if var_request.is_satisfied_by(&package).is_ok() ^ inverse {
                                    selected.push(*candidate);
                                }
                            } else if inverse {
                                // If reading the package failed but inverse is true, should
                                // we include the package as a candidate? Unclear.
                                selected.push(*candidate);
                            }
                        }
                        None => match &var_request.value {
                            spk_schema::ident::PinnableValue::FromBuildEnv => todo!(),
                            spk_schema::ident::PinnableValue::FromBuildEnvIfPresent => todo!(),
                            spk_schema::ident::PinnableValue::Pinned(value) => {
                                let SpkSolvable::GlobalVar {
                                    key: record_key,
                                    value: record_value,
                                } = &solvable.record
                                else {
                                    if inverse {
                                        selected.push(*candidate);
                                    }
                                    continue;
                                };
                                if (var_request.var.base_name() == record_key
                                    && value == record_value)
                                    ^ inverse
                                {
                                    selected.push(*candidate);
                                }
                            }
                        },
                    }
                }
                RequestVS::GlobalVar { key, value } => {
                    let SpkSolvable::GlobalVar {
                        key: record_key,
                        value: record_value,
                    } = &solvable.record
                    else {
                        if inverse {
                            selected.push(*candidate);
                        }
                        continue;
                    };
                    if (key == record_key && value == record_value) ^ inverse {
                        selected.push(*candidate);
                    }
                }
            }
        }
        selected
    }

    async fn get_candidates(&self, name: NameId) -> Option<Candidates> {
        let pkg_name = self.pool.resolve_package_name(name);

        if let Some(key) = pkg_name.name.strip_prefix(PSEUDO_PKG_NAME_PREFIX) {
            self.queried_global_var_values
                .borrow_mut()
                .insert(key.to_owned());

            if let Some(values) = self.known_global_var_values.borrow().get(key) {
                let mut candidates = Candidates {
                    candidates: Vec::with_capacity(values.len()),
                    ..Default::default()
                };
                for value in values {
                    let solvable_id = *self
                        .interned_solvables
                        .borrow_mut()
                        .entry(SpkSolvable::GlobalVar {
                            key: key.to_owned(),
                            value: value.clone(),
                        })
                        .or_insert_with(|| {
                            self.pool.intern_solvable(
                                name,
                                SpkSolvable::GlobalVar {
                                    key: key.to_owned(),
                                    value: value.clone(),
                                },
                            )
                        });
                    candidates.candidates.push(solvable_id);
                }
                return Some(candidates);
            }
            return None;
        }

        let mut located_builds = Vec::new();

        for repo in &self.repos {
            let versions = repo
                .list_package_versions(&pkg_name.name)
                .await
                .unwrap_or_default();
            for version in versions.iter() {
                // TODO: We need a borrowing version of this to avoid cloning.
                let pkg_version = VersionIdent::new(pkg_name.name.clone(), (**version).clone());

                let builds = repo
                    .list_package_builds(&pkg_version)
                    .await
                    .unwrap_or_default();

                for build in builds {
                    let located_build_ident =
                        LocatedBuildIdent::new(repo.name().to_owned(), build.clone());
                    if let SyntheticComponent::Actual(pkg_name_component) = &pkg_name.component {
                        let components = if let Build::Embedded(EmbeddedSource::Package(_parent)) =
                            build.build()
                        {
                            // Does this embedded stub contain the component
                            // being requested? For whatever reason,
                            // list_build_components returns an empty list for
                            // embedded stubs.
                            itertools::Either::Right(
                                if let Ok(stub) = repo.read_embed_stub(&build).await {
                                    itertools::Either::Right(
                                        stub.components()
                                            .iter()
                                            .map(|component_spec| component_spec.name.clone())
                                            .collect::<Vec<_>>()
                                            .into_iter(),
                                    )
                                } else {
                                    itertools::Either::Left(std::iter::empty())
                                },
                            )
                        } else {
                            itertools::Either::Left(
                                repo.list_build_components(&build).await.unwrap_or_default(),
                            )
                        };
                        for component in components.into_iter().chain(
                            // A build representing the All component is included so
                            // when a request for it is found it can act as a
                            // surrogate that depends on all the individual
                            // components.
                            [Component::All],
                        ) {
                            if component != *pkg_name_component
                                && (self.binary_only || !component.is_source())
                            {
                                continue;
                            }

                            located_builds.push(LocatedBuildIdentWithComponent {
                                ident: located_build_ident.clone(),
                                component: pkg_name.component.clone(),
                                requires_build_from_source: component != *pkg_name_component,
                            });
                        }
                    } else {
                        located_builds.push(LocatedBuildIdentWithComponent {
                            ident: located_build_ident,
                            component: SyntheticComponent::Base,
                            requires_build_from_source: false,
                        });
                    }
                }
            }
        }

        if located_builds.is_empty() {
            return None;
        }

        let mut candidates = Candidates {
            candidates: Vec::with_capacity(located_builds.len()),
            ..Default::default()
        };

        for build in located_builds {
            // What we need from build before it is moved into the pool.
            let ident = build.ident.clone();
            let requires_build_from_source = build.requires_build_from_source;

            let solvable_id = *self
                .interned_solvables
                .borrow_mut()
                .entry(SpkSolvable::LocatedBuildIdentWithComponent(build.clone()))
                .or_insert_with(|| {
                    self.pool
                        .intern_solvable(name, SpkSolvable::LocatedBuildIdentWithComponent(build))
                });

            // Filter builds that don't conform to global options
            // XXX: This find runtime will add up.
            let repo = self
                .repos
                .iter()
                .find(|repo| repo.name() == ident.repository_name())
                .expect("Expected solved package's repository to be in the list of repositories");

            if requires_build_from_source {
                match self.can_build_from_source(&ident).await {
                    CanBuildFromSource::Yes => {
                        candidates.candidates.push(solvable_id);
                    }
                    CanBuildFromSource::No(reason) => {
                        candidates.excluded.push((solvable_id, reason));
                    }
                }
                continue;
            }

            match repo.read_package(ident.target()).await {
                Ok(package) => {
                    // Filter builds that don't satisfy global var requests
                    if let Some(VarRequest {
                        value: PinnableValue::Pinned(expected_version),
                        ..
                    }) = self.global_var_requests.get(ident.name().as_opt_name())
                    {
                        if let Ok(expected_version) = parse_version_range(expected_version) {
                            if let spk_schema::version::Compatibility::Incompatible(
                                incompatible_reason,
                            ) = expected_version.is_applicable(package.version())
                            {
                                candidates.excluded.push((
                                solvable_id,
                                self.pool.intern_string(format!(
                                    "build version does not satisfy global var request: {incompatible_reason}"
                                )),
                            ));
                                continue;
                            }
                        }
                    }

                    // XXX: `package.check_satisfies_request` walks the
                    // package's build options, so is it better to do this loop
                    // over `option_values` here, or loop over all the
                    // global_var_requests instead?
                    for (opt_name, _value) in package.option_values() {
                        if let Some(request) = self.global_var_requests.get(&opt_name) {
                            if let spk_schema::version::Compatibility::Incompatible(
                                incompatible_reason,
                            ) = package.check_satisfies_request(request)
                            {
                                candidates.excluded.push((
                                        solvable_id,
                                        self.pool.intern_string(format!(
                                            "build option {opt_name} does not satisfy global var request: {incompatible_reason}"
                                        )),
                                    ));
                                continue;
                            }
                        }
                    }

                    candidates.candidates.push(solvable_id);
                }
                Err(err) => {
                    candidates
                        .excluded
                        .push((solvable_id, self.pool.intern_string(err.to_string())));
                }
            }
        }

        Some(candidates)
    }

    async fn sort_candidates(&self, _solver: &SolverCache<Self>, solvables: &mut [SolvableId]) {
        // This implementation just picks the highest version.
        // TODO: The ordering should take component names into account, so
        // the run component or the build component is tried first in the
        // appropriate situations.
        solvables.sort_by(|a, b| {
            let a = self.pool.resolve_solvable(*a);
            let b = self.pool.resolve_solvable(*b);
            match (&a.record, &b.record) {
                (
                    SpkSolvable::LocatedBuildIdentWithComponent(a),
                    SpkSolvable::LocatedBuildIdentWithComponent(b),
                ) => match b.ident.version().cmp(a.ident.version()) {
                    std::cmp::Ordering::Equal => {
                        // Sort source builds last
                        match (a.ident.build(), b.ident.build()) {
                            (Build::Source, Build::Source) => std::cmp::Ordering::Equal,
                            (Build::Source, _) => std::cmp::Ordering::Greater,
                            (_, Build::Source) => std::cmp::Ordering::Less,
                            _ => std::cmp::Ordering::Equal,
                        }
                    }
                    ord => ord,
                },
                (
                    SpkSolvable::GlobalVar {
                        key: a_key,
                        value: a_value,
                    },
                    SpkSolvable::GlobalVar {
                        key: b_key,
                        value: b_value,
                    },
                ) => {
                    if a_key == b_key {
                        a_value.cmp(b_value)
                    } else {
                        a_key.cmp(b_key)
                    }
                }
                (SpkSolvable::LocatedBuildIdentWithComponent(_), SpkSolvable::GlobalVar { .. }) => {
                    std::cmp::Ordering::Less
                }
                (SpkSolvable::GlobalVar { .. }, SpkSolvable::LocatedBuildIdentWithComponent(_)) => {
                    std::cmp::Ordering::Greater
                }
            }
        });
    }

    async fn get_dependencies(&self, solvable: SolvableId) -> Dependencies {
        let solvable = self.pool.resolve_solvable(solvable);
        let SpkSolvable::LocatedBuildIdentWithComponent(located_build_ident_with_component) =
            &solvable.record
        else {
            return Dependencies::Known(KnownDependencies::default());
        };
        let actual_component = match &located_build_ident_with_component.component {
            SyntheticComponent::Base => {
                // Base can't depend on anything because we don't know what
                // components actually exist or if requests exist for whatever it
                // was we picked if we were to pick a component to depend on.
                return Dependencies::Known(KnownDependencies::default());
            }
            SyntheticComponent::Actual(component) => component,
        };
        // XXX: This find runtime will add up.
        let repo = self
            .repos
            .iter()
            .find(|repo| repo.name() == located_build_ident_with_component.ident.repository_name())
            .expect("Expected solved package's repository to be in the list of repositories");
        match repo
            .read_package(located_build_ident_with_component.ident.target())
            .await
        {
            Ok(package) => {
                let mut known_deps = KnownDependencies {
                    requirements: Vec::with_capacity(package.runtime_requirements().len()),
                    // This is where IfAlreadyPresent constraints would go.
                    constrains: Vec::with_capacity(package.get_build_options().len()),
                };
                if located_build_ident_with_component.component.is_all() {
                    // The only dependencies of the All component are the other
                    // components defined in the package.
                    for component_spec in package.components().iter() {
                        let dep_name = self.pool.intern_package_name(PkgNameBufWithComponent {
                            name: located_build_ident_with_component.ident.name().to_owned(),
                            component: SyntheticComponent::Actual(component_spec.name.clone()),
                        });
                        known_deps.requirements.push(
                            self.pool
                                .intern_version_set(
                                    dep_name,
                                    RequestVS::SpkRequest(
                                        located_build_ident_with_component
                                            .as_request_with_components([component_spec
                                                .name
                                                .clone()]),
                                    ),
                                )
                                .into(),
                        );
                    }
                    return Dependencies::Known(known_deps);
                } else {
                    // For any non-All/non-Base component, add a dependency on
                    // the base to ensure all components come from the same
                    // base version.
                    let dep_name = self.pool.intern_package_name(PkgNameBufWithComponent {
                        name: located_build_ident_with_component.ident.name().to_owned(),
                        component: SyntheticComponent::Base,
                    });
                    known_deps.requirements.push(
                        self.pool
                            .intern_version_set(
                                dep_name,
                                RequestVS::SpkRequest(
                                    located_build_ident_with_component
                                        .as_request_with_components([]),
                                ),
                            )
                            .into(),
                    );
                    // Also add dependencies on any components that this
                    // component "uses" and its install requirements.
                    if let Some(component_spec) = package
                        .components()
                        .iter()
                        .find(|component_spec| component_spec.name == *actual_component)
                    {
                        component_spec.uses.iter().for_each(|uses| {
                            let dep_name = self.pool.intern_package_name(PkgNameBufWithComponent {
                                name: located_build_ident_with_component.ident.name().to_owned(),
                                component: SyntheticComponent::Actual(uses.clone()),
                            });
                            known_deps.requirements.push(
                                self.pool
                                    .intern_version_set(
                                        dep_name,
                                        RequestVS::SpkRequest(
                                            located_build_ident_with_component
                                                .as_request_with_components([uses.clone()]),
                                        ),
                                    )
                                    .into(),
                            );
                        });
                        known_deps
                            .requirements
                            .extend(self.pkg_requirements(&component_spec.requirements));
                    }
                }
                // Also add dependencies on any packages embedded in this
                // component.
                for embedded in package.embedded().iter() {
                    // If this embedded package is configured to exist in
                    // specific components, then skip it if this solvable's
                    // component is not one of those.
                    let components_where_this_embedded_package_exists = package
                        .components()
                        .iter()
                        .filter_map(|component_spec| {
                            if component_spec.embedded.iter().any(|embedded_package| {
                                embedded_package.pkg.name() == embedded.name()
                                    && embedded_package
                                        .pkg
                                        .target()
                                        .as_ref()
                                        .map(|version| version == embedded.version())
                                        .unwrap_or(true)
                            }) {
                                Some(component_spec.name.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<BTreeSet<_>>();
                    if !components_where_this_embedded_package_exists.is_empty()
                        && !components_where_this_embedded_package_exists.contains(actual_component)
                    {
                        continue;
                    }

                    let dep_name = self.pool.intern_package_name(PkgNameBufWithComponent {
                        name: embedded.name().to_owned(),
                        component: located_build_ident_with_component.component.clone(),
                    });
                    known_deps.requirements.push(
                        self.pool
                            .intern_version_set(
                                dep_name,
                                RequestVS::SpkRequest(Request::Pkg(PkgRequest::new(
                                    RangeIdent {
                                        repository_name: Some(
                                            located_build_ident_with_component
                                                .ident
                                                .repository_name()
                                                .to_owned(),
                                        ),
                                        name: embedded.name().to_owned(),
                                        components: Default::default(),
                                        version: VersionFilter::single(
                                            DoubleEqualsVersion::version_range(
                                                embedded.version().clone(),
                                            ),
                                        ),
                                        // This needs to match the build of
                                        // the stub for get_candidates to like
                                        // it. Stub parents are always the Run
                                        // component.
                                        build: Some(Build::Embedded(EmbeddedSource::Package(
                                            Box::new(EmbeddedSourcePackage {
                                                ident: package.ident().into(),
                                                components: BTreeSet::from_iter([Component::Run]),
                                            }),
                                        ))),
                                    },
                                    RequestedBy::Embedded(
                                        located_build_ident_with_component.ident.target().clone(),
                                    ),
                                ))),
                            )
                            .into(),
                    );
                    // Any install requirements of components inside embedded
                    // packages with the same name as this component also
                    // become dependencies.
                    for embedded_component_requirement in embedded
                        .components()
                        .iter()
                        .filter(|embedded_component| embedded_component.name == *actual_component)
                        .flat_map(|embedded_component| embedded_component.requirements.iter())
                    {
                        let kd = self.request_to_known_dependencies(embedded_component_requirement);
                        known_deps.requirements.extend(kd.requirements);
                        known_deps.constrains.extend(kd.constrains);
                    }
                }
                // If this solvable is an embedded stub and it is
                // representing that it provides a component that lives in a
                // component of the parent, then that parent component needs
                // to be included in the solution.
                if let Build::Embedded(EmbeddedSource::Package(parent)) =
                    located_build_ident_with_component.ident.build()
                {
                    match actual_component {
                        Component::Run => {
                            // The Run component is the default "home" of
                            // embedded packages, no dependency needed in this
                            // case.
                        }
                        component => 'invalid_parent: {
                            // XXX: Do we not have a convenient way to read the
                            // parent package from an embedded stub ident?
                            let Ok(pkg_name) = PkgNameBuf::from_str(&parent.ident.pkg_name) else {
                                break 'invalid_parent;
                            };
                            let Some(version_str) = parent.ident.version_str.as_ref() else {
                                break 'invalid_parent;
                            };
                            let Ok(version) = Version::from_str(version_str) else {
                                break 'invalid_parent;
                            };
                            let Some(build_str) = parent.ident.build_str.as_ref() else {
                                break 'invalid_parent;
                            };
                            let Ok(build) = Build::from_str(build_str) else {
                                break 'invalid_parent;
                            };
                            let ident = BuildIdent::new(
                                VersionIdent::new(pkg_name, version),
                                build.clone(),
                            );
                            let Ok(parent) = repo.read_package(&ident).await else {
                                break 'invalid_parent;
                            };
                            // Look through the components of the parent to see
                            // if one (or more?) of them embeds this component.
                            for parent_component in parent.components().iter() {
                                parent_component
                                    .embedded
                                    .iter()
                                    .filter(|embedded_package| {
                                        embedded_package.pkg.name()
                                            == located_build_ident_with_component.ident.name()
                                            && embedded_package
                                                .pkg
                                                .target()
                                                .as_ref()
                                                .map(|version| {
                                                    version
                                                        == located_build_ident_with_component
                                                            .ident
                                                            .version()
                                                })
                                                .unwrap_or(true)
                                            && embedded_package.components().contains(component)
                                    })
                                    .for_each(|_embedded_package| {
                                        let dep_name = self.pool.intern_package_name(
                                            PkgNameBufWithComponent {
                                                name: ident.name().to_owned(),
                                                component: SyntheticComponent::Actual(
                                                    parent_component.name.clone(),
                                                ),
                                            },
                                        );
                                        known_deps.requirements.push(
                                            self.pool.intern_version_set(
                                                dep_name,
                                                RequestVS::SpkRequest(Request::Pkg(
                                                    PkgRequest::new(
                                                        RangeIdent {
                                                            repository_name: Some(
                                                                located_build_ident_with_component
                                                                    .ident
                                                                    .repository_name()
                                                                    .to_owned(),
                                                            ),
                                                            name: ident.name().to_owned(),
                                                            components: BTreeSet::from_iter([
                                                                parent_component.name.clone(),
                                                            ]),
                                                            version: VersionFilter::single(
                                                                DoubleEqualsVersion::version_range(
                                                                    ident.version().clone(),
                                                                ),
                                                            ),
                                                            build: Some(build.clone()),
                                                        },
                                                        RequestedBy::Embedded(
                                                            located_build_ident_with_component
                                                                .ident
                                                                .target()
                                                                .clone(),
                                                        ),
                                                    ),
                                                ))
                                            ).into(),
                                        );
                                    });
                            }
                        }
                    }
                }
                for option in package.get_build_options() {
                    let Opt::Var(var_opt) = option else {
                        continue;
                    };
                    if var_opt.var.namespace().is_some() {
                        continue;
                    }
                    let Some(value) = var_opt.get_value(None) else {
                        continue;
                    };
                    let pseudo_pkg_name =
                        format!("{PSEUDO_PKG_NAME_PREFIX}{}", var_opt.var.base_name());
                    if self
                        .known_global_var_values
                        .borrow_mut()
                        .entry(var_opt.var.base_name().to_owned())
                        .or_default()
                        .insert(VarValue::Owned(value.clone()))
                        && self
                            .queried_global_var_values
                            .borrow()
                            .contains(var_opt.var.base_name())
                    {
                        // Seeing a new value for a var that has already locked
                        // in the list of candidates.
                        self.cancel_solving.set(true);
                    }
                    let dep_name = self.pool.intern_package_name(PkgNameBufWithComponent {
                        name: pkg_name!(&pseudo_pkg_name).to_owned(),
                        component: SyntheticComponent::Base,
                    });
                    // Add a constraint not a dependency because the package
                    // is targeting a specific global var value but there may
                    // not be a request for that var of a specific value.
                    known_deps.constrains.push(self.pool.intern_version_set(
                        dep_name,
                        RequestVS::GlobalVar {
                            key: var_opt.var.base_name().to_owned(),
                            value: VarValue::Owned(value),
                        },
                    ));
                }
                for requirement in package.runtime_requirements().iter() {
                    let kd = self.request_to_known_dependencies(requirement);
                    known_deps.requirements.extend(kd.requirements);
                    known_deps.constrains.extend(kd.constrains);
                }
                Dependencies::Known(known_deps)
            }
            Err(err) => {
                let msg = self.pool.intern_string(err.to_string());
                Dependencies::Unknown(msg)
            }
        }
    }

    fn should_cancel_with_value(&self) -> Option<Box<dyn std::any::Any>> {
        if self.cancel_solving.get() {
            // Eventually there will be more than one reason the solve is
            // cancelled...
            Some(Box::new(()))
        } else {
            None
        }
    }
}

impl Interner for SpkProvider {
    fn display_solvable(&self, solvable: SolvableId) -> impl std::fmt::Display + '_ {
        let solvable = self.pool.resolve_solvable(solvable);
        format!("{}", solvable.record)
    }

    fn display_name(&self, name: NameId) -> impl std::fmt::Display + '_ {
        self.pool.resolve_package_name(name)
    }

    fn display_version_set(&self, version_set: VersionSetId) -> impl std::fmt::Display + '_ {
        self.pool.resolve_version_set(version_set)
    }

    fn display_string(&self, string_id: StringId) -> impl std::fmt::Display + '_ {
        self.pool.resolve_string(string_id)
    }

    fn version_set_name(&self, version_set: VersionSetId) -> NameId {
        self.pool.resolve_version_set_package_name(version_set)
    }

    fn solvable_name(&self, solvable: SolvableId) -> NameId {
        self.pool.resolve_solvable(solvable).name
    }

    fn version_sets_in_union(
        &self,
        version_set_union: VersionSetUnionId,
    ) -> impl Iterator<Item = VersionSetId> {
        self.pool.resolve_version_set_union(version_set_union)
    }
}
