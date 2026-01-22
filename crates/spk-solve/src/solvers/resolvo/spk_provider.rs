// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Not;
use std::sync::Arc;

use itertools::Itertools;
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
use spk_schema::ident::{
    LocatedBuildIdent,
    PinnedValue,
    PkgRequest,
    PkgRequestOptionValue,
    PkgRequestOptions,
    PkgRequestWithOptions,
    PreReleasePolicy,
    RangeIdent,
    RequestWithOptions,
    RequestedBy,
    Satisfy,
    VarRequest,
};
use spk_schema::ident_build::{Build, EmbeddedSource, EmbeddedSourcePackage};
use spk_schema::ident_component::Component;
use spk_schema::name::{OptNameBuf, PkgNameBuf};
use spk_schema::prelude::{HasVersion, Named};
use spk_schema::version_range::{DoubleEqualsVersion, Ranged, VersionFilter, parse_version_range};
use spk_schema::{
    BuildIdent,
    Components,
    Deprecate,
    Opt,
    OptionValues,
    Package,
    Recipe,
    Spec,
    VersionIdent,
};
use spk_solve_package_iterator::{BuildKey, BuildToSortedOptName, SortedBuildIterator};
use spk_storage::RepositoryHandle;
use tracing::{Instrument, debug_span};

use super::pkg_request_version_set::{
    LocatedBuildIdentWithComponent,
    RequestVS,
    SpkSolvable,
    SyntheticComponent,
    VarValue,
};
use crate::SolverMut;

// Using just the package name as a Resolvo "package name" prevents multiple
// components from the same package from existing in the same solution, since
// we consider the different components to be different "solvables". Instead,
// treat different components of a package as separate packages. There needs to
// be a relationship between every component of a package and a "base"
// component, to prevent a solve containing a mix of components from different
// versions of the same package.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) enum ResolvoPackageName {
    GlobalVar(OptNameBuf),
    PkgNameBufWithComponent(PkgNameBufWithComponent),
}

impl ResolvoPackageName {
    async fn get_candidates(&self, name: NameId, provider: &SpkProvider) -> Option<Candidates> {
        match self {
            ResolvoPackageName::GlobalVar(key) => {
                provider
                    .queried_global_var_values
                    .borrow_mut()
                    .insert(key.clone());

                if let Some(values) = provider.known_global_var_values.borrow().get(key) {
                    let mut candidates = Candidates {
                        candidates: Vec::with_capacity(values.len()),
                        ..Default::default()
                    };
                    for value in values {
                        let solvable_id = *provider
                            .interned_solvables
                            .borrow_mut()
                            .entry(SpkSolvable::GlobalVar {
                                key: key.clone(),
                                value: value.clone(),
                            })
                            .or_insert_with(|| {
                                provider.pool.intern_solvable(
                                    name,
                                    SpkSolvable::GlobalVar {
                                        key: key.clone(),
                                        value: value.clone(),
                                    },
                                )
                            });
                        candidates.candidates.push(solvable_id);
                    }
                    return Some(candidates);
                }
                None
            }
            ResolvoPackageName::PkgNameBufWithComponent(pkg_name) => {
                // Prevent duplicate solvables by using a set. Ties (same build
                // but "requires_build_from_source" differs) are resolved with
                // "requires_build_from_source" being true.
                let mut located_builds = HashSet::new();

                let root_pkg_request = provider.global_pkg_requests.get(&pkg_name.name);

                for repo in &provider.repos {
                    let versions = repo
                        .list_package_versions(&pkg_name.name)
                        .await
                        .unwrap_or_default();
                    for version in versions.iter() {
                        // Skip versions known to be excluded to avoid the
                        // overhead of reading all the builds and their package
                        // specs. The tradeoff is that resolvo doesn't learn
                        // these builds exist to use them when constructing
                        // error messages. However from observation solver
                        // errors already don't mention versions that aren't
                        // applicable to root requests.
                        if let Some(pkg_request) = root_pkg_request
                            && !pkg_request.pkg.version.is_applicable(version).is_ok()
                        {
                            continue;
                        }

                        // TODO: We need a borrowing version of this to avoid cloning.
                        let pkg_version =
                            VersionIdent::new(pkg_name.name.clone(), (**version).clone());

                        let builds = repo
                            .list_package_builds(&pkg_version)
                            .await
                            .unwrap_or_default();

                        for build in builds {
                            let located_build_ident =
                                LocatedBuildIdent::new(repo.name().to_owned(), build.clone());
                            if let SyntheticComponent::Actual(pkg_name_component) =
                                &pkg_name.component
                            {
                                let components =
                                    if let Build::Embedded(EmbeddedSource::Package(_parent)) =
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
                                                        .map(|component_spec| {
                                                            component_spec.name.clone()
                                                        })
                                                        .collect::<Vec<_>>()
                                                        .into_iter(),
                                                )
                                            } else {
                                                itertools::Either::Left(std::iter::empty())
                                            },
                                        )
                                    } else {
                                        itertools::Either::Left(
                                            repo.list_build_components(&build)
                                                .await
                                                .unwrap_or_default(),
                                        )
                                    };
                                for component in components.into_iter().chain(
                                    // A build representing the All component is included so
                                    // when a request for it is found it can act as a
                                    // surrogate that depends on all the individual
                                    // components.
                                    {
                                        if !build.is_source() {
                                            itertools::Either::Left([Component::All].into_iter())
                                        } else {
                                            // XXX: Unclear if this is the right
                                            // approach but without this special
                                            // case the Solution can incorrectly
                                            // end up with a src build marked as
                                            // requires_build_from_source for
                                            // requests that are asking for
                                            // :src.
                                            itertools::Either::Right([].into_iter())
                                        }
                                    },
                                ) {
                                    let requires_build_from_source = build.is_source()
                                        && (component != *pkg_name_component
                                            || pkg_name_component.is_all());

                                    if requires_build_from_source && provider.binary_only {
                                        // Deny anything that requires build
                                        // from source when binary_only is
                                        // enabled.
                                        continue;
                                    }

                                    if (!requires_build_from_source || !build.is_source())
                                        && component != *pkg_name_component
                                    {
                                        // Deny components that don't match
                                        // unless it is possible to build from
                                        // source.
                                        continue;
                                    }

                                    let new_entry = LocatedBuildIdentWithComponent {
                                        ident: located_build_ident.clone(),
                                        component: pkg_name.component.clone(),
                                        requires_build_from_source,
                                    };

                                    if requires_build_from_source {
                                        // _replace_ any existing entry, which
                                        // might have
                                        // requires_build_from_source == false,
                                        // so it now is true.
                                        located_builds.replace(new_entry);
                                    } else {
                                        // _insert_ to not overwrite any
                                        // existing entry that might have
                                        // requires_build_from_source == true.
                                        located_builds.insert(new_entry);
                                    }
                                }
                            } else {
                                located_builds.insert(LocatedBuildIdentWithComponent {
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

                    let solvable_id = *provider
                        .interned_solvables
                        .borrow_mut()
                        .entry(SpkSolvable::LocatedBuildIdentWithComponent(build.clone()))
                        .or_insert_with(|| {
                            provider.pool.intern_solvable(
                                name,
                                SpkSolvable::LocatedBuildIdentWithComponent(build),
                            )
                        });

                    // Filter builds that don't conform to global options
                    // XXX: This find runtime will add up.
                    let repo = provider
                .repos
                .iter()
                .find(|repo| repo.name() == ident.repository_name())
                .expect("Expected solved package's repository to be in the list of repositories");

                    if requires_build_from_source {
                        match provider.can_build_from_source(&ident).await {
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
                            if let Some(VarRequest::<PinnedValue> {
                                value: expected_version,
                                ..
                            }) = provider.global_var_requests.get(ident.name().as_opt_name())
                                && let Ok(expected_version) = parse_version_range(expected_version)
                                && let spk_schema::version::Compatibility::Incompatible(
                                    incompatible_reason,
                                ) = expected_version.is_applicable(package.version())
                            {
                                candidates.excluded.push((
                                solvable_id,
                                provider.pool.intern_string(format!(
                                    "build version does not satisfy global var request: {incompatible_reason}"
                                )),
                            ));
                                continue;
                            }

                            // XXX: `package.check_satisfies_request` walks the
                            // package's build options, so is it better to do this loop
                            // over `option_values` here, or loop over all the
                            // global_var_requests instead?
                            for (opt_name, _value) in package.option_values() {
                                if let Some(request) = provider.global_var_requests.get(&opt_name)
                                    && let spk_schema::version::Compatibility::Incompatible(
                                        incompatible_reason,
                                    ) = package.check_satisfies_request(request)
                                {
                                    candidates.excluded.push((
                                        solvable_id,
                                        provider.pool.intern_string(format!(
                                            "build option {opt_name} does not satisfy global var request: {incompatible_reason}"
                                        )),
                                    ));
                                    continue;
                                }
                            }

                            candidates.candidates.push(solvable_id);
                        }
                        Err(err) => {
                            candidates
                                .excluded
                                .push((solvable_id, provider.pool.intern_string(err.to_string())));
                        }
                    }
                }

                Some(candidates)
            }
        }
    }
}

impl std::fmt::Display for ResolvoPackageName {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ResolvoPackageName::GlobalVar(name) => write!(f, "{name}"),
            ResolvoPackageName::PkgNameBufWithComponent(name) => write!(f, "{name}"),
        }
    }
}

enum CanBuildFromSource {
    Yes,
    No(StringId),
}

/// An iterator that yields slices of items that fall into the same partition.
///
/// The partition is determined by the key function.
/// The items must be already sorted in ascending order by the key function.
struct PartitionIter<'a, I, F, K>
where
    F: for<'i> Fn(&'i I) -> K,
    K: PartialOrd,
{
    slice: &'a [I],
    key_fn: F,
}

impl<'a, I, F, K> PartitionIter<'a, I, F, K>
where
    F: for<'i> Fn(&'i I) -> K,
    K: PartialOrd,
{
    fn new(slice: &'a [I], key_fn: F) -> Self {
        Self { slice, key_fn }
    }
}

impl<'a, I, F, K> Iterator for PartitionIter<'a, I, F, K>
where
    F: for<'i> Fn(&'i I) -> K,
    K: PartialOrd,
{
    type Item = &'a [I];

    fn next(&mut self) -> Option<Self::Item> {
        let element = self.slice.first()?;

        // Is a binary search overkill?
        let partition_key = (self.key_fn)(element);
        // No need to check the first element again.
        let p =
            1 + self.slice[1..].partition_point(|element| (self.key_fn)(element) <= partition_key);

        let part = &self.slice[..p];
        self.slice = &self.slice[p..];
        Some(part)
    }
}

pub(crate) struct SpkProvider {
    pub(crate) pool: Pool<RequestVS, ResolvoPackageName>,
    repos: Vec<Arc<RepositoryHandle>>,
    /// Global package requests. These can be used to constrain the candidates
    /// returned for these packages.
    global_pkg_requests: HashMap<PkgNameBuf, PkgRequestWithOptions>,
    /// Global options, like what might be specified with `--opt` to `spk env`.
    /// Indexed by name. If multiple requests happen to exist with the same
    /// name, the last one is kept.
    global_var_requests: HashMap<OptNameBuf, VarRequest<PinnedValue>>,
    interned_solvables: RefCell<HashMap<SpkSolvable, SolvableId>>,
    /// Track all the global var keys and values that have been witnessed while
    /// solving.
    known_global_var_values: RefCell<HashMap<OptNameBuf, HashSet<VarValue>>>,
    /// Track which global var candidates have been queried by the solver. Once
    /// queried, it is no longer possible to add more possible values without
    /// restarting the solve.
    queried_global_var_values: RefCell<HashSet<OptNameBuf>>,
    cancel_solving: RefCell<Option<String>>,
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

            for request in build_requirements.iter().cloned() {
                solver.add_request(request);
            }

            // These are last to take priority over the requests in the recipe.
            for request in self.global_var_requests.values() {
                solver.add_request(RequestWithOptions::Var(request.clone()));
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
            global_pkg_requests: Default::default(),
            global_var_requests: Default::default(),
            interned_solvables: Default::default(),
            known_global_var_values: Default::default(),
            queried_global_var_values: Default::default(),
            cancel_solving: Default::default(),
            binary_only,
            build_from_source_trail: RefCell::new(build_from_source_trail),
        }
    }

    fn pkg_request_to_known_dependencies(
        &self,
        pkg_request: &PkgRequestWithOptions,
    ) -> KnownDependencies {
        let mut components = pkg_request.pkg.components.iter().peekable();
        let iter = if components.peek().is_some() {
            itertools::Either::Right(components.cloned())
        } else {
            itertools::Either::Left(
                // A request with no components is assumed to be a request for
                // the default_for_run (or source) component.
                if pkg_request
                    .pkg
                    .build
                    .as_ref()
                    .map(|build| build.is_source())
                    .unwrap_or(false)
                {
                    std::iter::once(Component::Source)
                } else {
                    std::iter::once(Component::default_for_run())
                },
            )
        };
        let mut known_deps = KnownDependencies {
            requirements: Vec::new(),
            constrains: Vec::new(),
        };
        for component in iter {
            let dep_name =
                self.pool
                    .intern_package_name(ResolvoPackageName::PkgNameBufWithComponent(
                        PkgNameBufWithComponent {
                            name: pkg_request.pkg.name().to_owned(),
                            component: SyntheticComponent::Actual(component.clone()),
                        },
                    ));
            let mut pkg_request_with_component = pkg_request.clone();
            // If this is a package that is allowed to be a prerelease via a
            // top-level request, then when it appears as a dependency of other
            // packages it needs to allow prereleases as well (unless it already
            // has an explicit prerelease policy).
            if pkg_request_with_component.prerelease_policy.is_none()
                && self
                    .global_pkg_requests
                    .get(pkg_request.pkg.name())
                    .and_then(|r| r.prerelease_policy)
                    .is_some_and(|p| p.is_include_all())
            {
                pkg_request_with_component.pkg_request.prerelease_policy =
                    Some(PreReleasePolicy::IncludeAll);
            }
            pkg_request_with_component.pkg.components = BTreeSet::from_iter([component]);
            let dep_vs = self.pool.intern_version_set(
                dep_name,
                RequestVS::SpkRequest(RequestWithOptions::Pkg(pkg_request_with_component)),
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

    /// Add any package requests found in the given requests to the global
    /// package requests, returning a list of Requirement.
    pub(crate) fn root_pkg_requirements(
        &mut self,
        requests: &[RequestWithOptions],
    ) -> Vec<Requirement> {
        self.global_pkg_requests.reserve(requests.len());
        requests
            .iter()
            .filter_map(|req| match req {
                RequestWithOptions::Pkg(pkg) => Some(pkg),
                _ => None,
            })
            .flat_map(|req| {
                self.global_pkg_requests
                    .insert(req.pkg.name().to_owned(), req.clone());
                self.pkg_request_to_known_dependencies(req).requirements
            })
            .collect()
    }

    /// Return a list of requirements for all the package requests found in the
    /// given requests.
    fn dep_pkg_requirements(&self, requests: &[RequestWithOptions]) -> Vec<Requirement> {
        requests
            .iter()
            .filter_map(|req| match req {
                RequestWithOptions::Pkg(pkg) => Some(pkg),
                _ => None,
            })
            .flat_map(|req| self.pkg_request_to_known_dependencies(req).requirements)
            .collect()
    }

    pub fn is_canceled(&self) -> bool {
        self.cancel_solving.borrow().is_some()
    }

    /// Return an iterator that yields slices of builds that are from the same
    /// package version.
    ///
    /// The provided builds must already be sorted otherwise the behavior is
    /// undefined.
    fn find_version_runs<'a>(
        builds: &'a [(SolvableId, &'a LocatedBuildIdentWithComponent, Arc<Spec>)],
    ) -> impl Iterator<Item = &'a [(SolvableId, &'a LocatedBuildIdentWithComponent, Arc<Spec>)]>
    {
        PartitionIter::new(builds, |(_, ident, _)| {
            // partition by (name, version) ignoring repository
            (ident.ident.name(), ident.ident.version())
        })
    }

    fn request_to_known_dependencies(&self, requirement: &RequestWithOptions) -> KnownDependencies {
        let mut known_deps = KnownDependencies::default();
        match requirement {
            RequestWithOptions::Pkg(pkg_request) => {
                let kd = self.pkg_request_to_known_dependencies(pkg_request);
                known_deps.requirements.extend(kd.requirements);
                known_deps.constrains.extend(kd.constrains);
            }
            RequestWithOptions::Var(var_request) => {
                let dep_name =
                    match var_request.var.namespace() {
                        Some(pkg_name) => self.pool.intern_package_name(
                            ResolvoPackageName::PkgNameBufWithComponent(PkgNameBufWithComponent {
                                name: pkg_name.to_owned(),
                                component: SyntheticComponent::Base,
                            }),
                        ),
                        None => {
                            // Since we will be adding constraints for
                            // global vars we need to add the pseudo-package
                            // to the dependency list so it will influence
                            // decisions.
                            if self
                                .known_global_var_values
                                .borrow_mut()
                                .entry(var_request.var.without_namespace().to_owned())
                                .or_default()
                                .insert(VarValue::ArcStr(Arc::clone(&var_request.value)))
                                && self
                                    .queried_global_var_values
                                    .borrow()
                                    .contains(var_request.var.without_namespace())
                            {
                                // Seeing a new value for a var that has
                                // already locked in the list of candidates.
                                *self.cancel_solving.borrow_mut() = Some(format!(
                                    "Saw new value for global var: {}/{}",
                                    var_request.var.without_namespace(),
                                    var_request.value
                                ));
                            }
                            let dep_name =
                                self.pool.intern_package_name(ResolvoPackageName::GlobalVar(
                                    var_request.var.without_namespace().to_owned(),
                                ));
                            known_deps.requirements.push(
                                self.pool
                                    .intern_version_set(
                                        dep_name,
                                        RequestVS::GlobalVar {
                                            key: var_request.var.without_namespace().to_owned(),
                                            value: VarValue::ArcStr(Arc::clone(&var_request.value)),
                                        },
                                    )
                                    .into(),
                            );
                            dep_name
                        }
                    };
                // If we end up adding pkg_name to the solve, it needs
                // to satisfy this var request.
                known_deps.constrains.push(
                    self.pool
                        .intern_version_set(dep_name, RequestVS::SpkRequest(requirement.clone())),
                );
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
            global_pkg_requests: self.global_pkg_requests.clone(),
            global_var_requests: self.global_var_requests.clone(),
            interned_solvables: Default::default(),
            known_global_var_values: RefCell::new(self.known_global_var_values.take()),
            queried_global_var_values: Default::default(),
            cancel_solving: Default::default(),
            binary_only: self.binary_only,
            build_from_source_trail: self.build_from_source_trail.clone(),
        }
    }

    /// Order two builds based on which should be preferred to include in a
    /// solve as a candidate.
    ///
    /// Generally this means a build with newer dependencies is ordered first.
    fn sort_builds(
        &self,
        build_key_index: &HashMap<SolvableId, BuildKey>,
        a: (SolvableId, &LocatedBuildIdentWithComponent),
        b: (SolvableId, &LocatedBuildIdentWithComponent),
    ) -> std::cmp::Ordering {
        // This function should _not_ return `std::cmp::Ordering::Equal` unless
        // `a` and `b` are the same build (in practice this function will never
        // be called when that is true).

        // Embedded stubs are always ordered last.
        match (a.1.ident.is_embedded(), b.1.ident.is_embedded()) {
            (true, false) => return std::cmp::Ordering::Greater,
            (false, true) => return std::cmp::Ordering::Less,
            _ => {}
        };

        match (build_key_index.get(&a.0), build_key_index.get(&b.0)) {
            (Some(a_key), Some(b_key)) => {
                // BuildKey orders in reverse order from what is needed here.
                return b_key.cmp(a_key);
            }
            (Some(_), None) => return std::cmp::Ordering::Less,
            (None, Some(_)) => return std::cmp::Ordering::Greater,
            _ => {}
        };

        // If neither build has a key, both packages failed to load?
        // Add debug assert to see if this ever happens.
        debug_assert!(false, "builds without keys {a:?} {b:?}");

        a.1.ident.cmp(&b.1.ident)
    }

    pub fn var_requirements(&mut self, requests: &[RequestWithOptions]) -> Vec<VersionSetId> {
        self.global_var_requests.reserve(requests.len());
        requests
            .iter()
            .filter_map(|req| match req {
                RequestWithOptions::Var(var) => Some(var),
                _ => None,
            })
            .filter_map(|req| match req.var.namespace() {
                Some(pkg_name) => {
                    // A global request applicable to a specific package.
                    let dep_name =
                        self.pool
                            .intern_package_name(ResolvoPackageName::PkgNameBufWithComponent(
                                PkgNameBufWithComponent {
                                    name: pkg_name.to_owned(),
                                    component: SyntheticComponent::Base,
                                },
                            ));
                    Some(self.pool.intern_version_set(
                        dep_name,
                        RequestVS::SpkRequest(RequestWithOptions::Var(req.clone())),
                    ))
                }
                None => {
                    // A global request affecting all packages.
                    self.global_var_requests
                        .insert(req.var.without_namespace().to_owned(), req.clone());
                    self.known_global_var_values
                        .borrow_mut()
                        .entry(req.var.without_namespace().to_owned())
                        .or_default()
                        .insert(VarValue::ArcStr(Arc::clone(&req.value)));
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
                RequestVS::SpkRequest(RequestWithOptions::Pkg(pkg_request_with_options)) => {
                    let PkgRequestWithOptions { pkg_request, .. } = pkg_request_with_options;

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
                        tracing::trace!(pkg_request = %pkg_request.pkg, version = %located_build_ident_with_component.ident.version(), "version applicable");
                        let is_source =
                            located_build_ident_with_component.ident.build().is_source();

                        // If build from source is enabled, any source build is
                        // a candidate. Source builds that can't be built from
                        // source are filtered out in `get_candidates`.
                        if located_build_ident_with_component.requires_build_from_source {
                            // However, building from source is not a suitable
                            // candidate for a request for a specific component
                            // of an existing build, such as when finding the
                            // members of the :all component of a build.
                            if pkg_request
                                .pkg
                                .build
                                .as_ref()
                                .is_some_and(|b| b.is_buildid())
                            {
                                if inverse {
                                    selected.push(*candidate);
                                }
                                continue;
                            }

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
                            // This can disqualify but not qualify; version
                            // compatibility check is still required.
                            if !pkg_request.pkg.components.contains(&Component::All) {
                                if inverse {
                                    selected.push(*candidate);
                                }
                                continue;
                            }
                        } else {
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
                            let compatibility = pkg_request_with_options.is_satisfied_by(&package);
                            let is_ok = compatibility.is_ok();
                            if is_ok {
                                tracing::trace!(pkg_request = %pkg_request.pkg, package = %package.ident(), %inverse, %compatibility, "satisfied by");
                            } else {
                                tracing::trace!(pkg_request = %pkg_request.pkg, package = %package.ident(), %inverse, %compatibility, "not satisfied by");
                            }
                            if is_ok ^ inverse {
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
                RequestVS::SpkRequest(RequestWithOptions::Var(var_request)) => {
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
                                let satisfied = var_request.is_satisfied_by(&package);
                                tracing::trace!(%var_request, package = %package.ident(), %satisfied, %inverse, "is_satisfied_by");

                                if satisfied.is_ok() ^ inverse {
                                    selected.push(*candidate);
                                }
                            } else if inverse {
                                // If reading the package failed but inverse is true, should
                                // we include the package as a candidate? Unclear.
                                selected.push(*candidate);
                            }
                        }
                        None => {
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
                            if (var_request.var.without_namespace() == record_key
                                && var_request.value == *record_value)
                                ^ inverse
                            {
                                selected.push(*candidate);
                            }
                        }
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
        let resolvo_package_name = self.pool.resolve_package_name(name);
        resolvo_package_name.get_candidates(name, self).await
    }

    async fn sort_candidates(&self, _solver: &SolverCache<Self>, solvables: &mut [SolvableId]) {
        // Goal: Create a `BuildKey` for each build in `solvables`.
        // The `BuildKey` factory needs as input the output from
        // `BuildToSortedOptName::sort_builds`.
        // `BuildToSortedOptName::sort_builds` needs to be fed builds from the
        // same version.
        // `solvables` can be builds from various versions so they need to be
        // grouped by version.
        let build_solvables = solvables
            .iter()
            .filter_map(|solvable_id| {
                let solvable = self.pool.resolve_solvable(*solvable_id);
                match &solvable.record {
                    SpkSolvable::LocatedBuildIdentWithComponent(
                        located_build_ident_with_component,
                    ) =>
                    // sorting the source build (if any) is handled
                    // elsewhere; skip source builds.
                    {
                        located_build_ident_with_component
                            .ident
                            .is_source()
                            .not()
                            .then_some((*solvable_id, located_build_ident_with_component))
                    }
                    _ => None,
                }
            })
            .sorted_by(
                |(_, LocatedBuildIdentWithComponent { ident: a, .. }),
                 (_, LocatedBuildIdentWithComponent { ident: b, .. })| {
                    // build_solvables will be ordered by (pkg, version, build).
                    a.target().cmp(b.target())
                },
            )
            .collect::<Vec<_>>();

        // `BuildToSortedOptName::sort_builds` will need the package specs.
        let mut build_solvables_and_specs = Vec::with_capacity(build_solvables.len());
        for build_solvable in build_solvables {
            let (solvable_id, located_build_ident_with_component) = build_solvable;
            let repo = self
                .repos
                .iter()
                .find(|repo| {
                    repo.name() == located_build_ident_with_component.ident.repository_name()
                })
                .expect("Expected solved package's repository to be in the list of repositories");
            let Ok(package) = repo
                .read_package(located_build_ident_with_component.ident.target())
                .await
            else {
                // Any builds that can't load the spec will be sorted to the
                // end. In most cases the package spec would already be loaded
                // in cache at this point.
                continue;
            };
            build_solvables_and_specs.push((
                solvable_id,
                located_build_ident_with_component,
                package,
            ));
        }

        let mut build_key_index = HashMap::new();
        build_key_index.reserve(build_solvables_and_specs.len());

        // Find runs of the same package version.
        for version_run in SpkProvider::find_version_runs(&build_solvables_and_specs) {
            let (ordered_names, build_name_values) =
                BuildToSortedOptName::sort_builds(version_run.iter().map(|(_, _, spec)| spec));

            for (solvable_id, _, spec) in version_run {
                let build_key = SortedBuildIterator::make_option_values_build_key(
                    spec,
                    &ordered_names,
                    &build_name_values,
                    false,
                );
                build_key_index.insert(*solvable_id, build_key);
            }
        }

        // TODO: The ordering should take component names into account, so
        // the run component or the build component is tried first in the
        // appropriate situations.
        solvables.sort_by(|solvable_id_a, solvable_id_b| {
            let a = self.pool.resolve_solvable(*solvable_id_a);
            let b = self.pool.resolve_solvable(*solvable_id_b);
            match (&a.record, &b.record) {
                (
                    SpkSolvable::LocatedBuildIdentWithComponent(a),
                    SpkSolvable::LocatedBuildIdentWithComponent(b),
                ) => {
                    // Sort source packages last to prefer using any existing
                    // build of whatever version over building from source.
                    match (a.ident.build(), b.ident.build()) {
                        (Build::Source, Build::Source) => {}
                        (Build::Source, _) => return std::cmp::Ordering::Greater,
                        (_, Build::Source) => return std::cmp::Ordering::Less,
                        _ => {}
                    };
                    // Sort embedded packages second last, even if an embedded
                    // package has the highest version.
                    match (a.ident.build(), b.ident.build()) {
                        (Build::Embedded(_), Build::Embedded(_)) => {}
                        (Build::Embedded(_), _) => return std::cmp::Ordering::Greater,
                        (_, Build::Embedded(_)) => return std::cmp::Ordering::Less,
                        _ => {}
                    };
                    // Then prefer higher versions...
                    match b.ident.version().cmp(a.ident.version()) {
                        std::cmp::Ordering::Equal => {
                            // Sort source builds last
                            match (a.ident.build(), b.ident.build()) {
                                (Build::Source, Build::Source) => {}
                                (Build::Source, _) => return std::cmp::Ordering::Greater,
                                (_, Build::Source) => return std::cmp::Ordering::Less,
                                _ => {}
                            };
                            self.sort_builds(
                                &build_key_index,
                                (*solvable_id_a, a),
                                (*solvable_id_b, b),
                            )
                        }
                        ord => ord,
                    }
                }
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
                        let dep_name = self.pool.intern_package_name(
                            ResolvoPackageName::PkgNameBufWithComponent(PkgNameBufWithComponent {
                                name: located_build_ident_with_component.ident.name().to_owned(),
                                component: SyntheticComponent::Actual(component_spec.name.clone()),
                            }),
                        );
                        known_deps.requirements.push(
                            self.pool
                                .intern_version_set(
                                    dep_name,
                                    RequestVS::SpkRequest(
                                        located_build_ident_with_component
                                            .as_request_with_components(
                                                &package,
                                                [component_spec.name.clone()],
                                            ),
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
                    let dep_name =
                        self.pool
                            .intern_package_name(ResolvoPackageName::PkgNameBufWithComponent(
                                PkgNameBufWithComponent {
                                    name: located_build_ident_with_component
                                        .ident
                                        .name()
                                        .to_owned(),
                                    component: SyntheticComponent::Base,
                                },
                            ));
                    known_deps.requirements.push(
                        self.pool
                            .intern_version_set(
                                dep_name,
                                RequestVS::SpkRequest(
                                    located_build_ident_with_component
                                        .as_request_with_components(&package, []),
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
                            let dep_name = self.pool.intern_package_name(
                                ResolvoPackageName::PkgNameBufWithComponent(
                                    PkgNameBufWithComponent {
                                        name: located_build_ident_with_component
                                            .ident
                                            .name()
                                            .to_owned(),
                                        component: SyntheticComponent::Actual(uses.clone()),
                                    },
                                ),
                            );
                            known_deps.requirements.push(
                                self.pool
                                    .intern_version_set(
                                        dep_name,
                                        RequestVS::SpkRequest(
                                            located_build_ident_with_component
                                                .as_request_with_components(
                                                    &package,
                                                    [uses.clone()],
                                                ),
                                        ),
                                    )
                                    .into(),
                            );
                        });
                        known_deps.requirements.extend(
                            self.dep_pkg_requirements(component_spec.requirements_with_options()),
                        );
                    }
                }
                // Also add dependencies on any packages embedded in this
                // component, unless this is a source package. Source packages
                // that get built from packages with embedded packages will also
                // claim to embed those packages, but this is meaningless.
                if !package.ident().is_source() {
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
                            && !components_where_this_embedded_package_exists
                                .contains(actual_component)
                        {
                            continue;
                        }

                        let dep_name = self.pool.intern_package_name(
                            ResolvoPackageName::PkgNameBufWithComponent(PkgNameBufWithComponent {
                                name: embedded.name().to_owned(),
                                component: located_build_ident_with_component.component.clone(),
                            }),
                        );
                        let options_for_embedded = PkgRequestOptions::from_iter(
                            package.runtime_requirements().iter().filter_map(|req| {
                                let var = req.var_ref()?;
                                (var.var.namespace() == Some(embedded.name())).then(|| {
                                    (
                                        var.var.clone(),
                                        PkgRequestOptionValue::Complete(var.value.to_string()),
                                    )
                                })
                            }),
                        );
                        known_deps.requirements.push(
                            self.pool
                                .intern_version_set(
                                    dep_name,
                                    RequestVS::SpkRequest(RequestWithOptions::Pkg(
                                        PkgRequestWithOptions {
                                            options: options_for_embedded,
                                            pkg_request: PkgRequest::new(
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
                                                    build: Some(Build::Embedded(
                                                        EmbeddedSource::Package(Box::new(
                                                            EmbeddedSourcePackage {
                                                                ident: package.ident().into(),
                                                                components: BTreeSet::from_iter([
                                                                    Component::Run,
                                                                ]),
                                                                unparsed: None,
                                                            },
                                                        )),
                                                    )),
                                                },
                                                RequestedBy::Embedded(
                                                    located_build_ident_with_component
                                                        .ident
                                                        .target()
                                                        .clone(),
                                                ),
                                            ),
                                        },
                                    )),
                                )
                                .into(),
                        );
                        // Any install requirements of components inside embedded
                        // packages with the same name as this component also
                        // become dependencies.
                        for embedded_component_requirement in embedded
                            .components()
                            .iter()
                            .filter(|embedded_component| {
                                embedded_component.name == *actual_component
                            })
                            .flat_map(|embedded_component| {
                                embedded_component.requirements_with_options().iter()
                            })
                        {
                            let kd =
                                self.request_to_known_dependencies(embedded_component_requirement);
                            known_deps.requirements.extend(kd.requirements);
                            known_deps.constrains.extend(kd.constrains);
                        }
                    }
                }
                // If this solvable is an embedded stub and it is
                // representing that it provides a component that lives in a
                // component of the parent, then that parent component needs
                // to be included in the solution.
                if let Build::Embedded(EmbeddedSource::Package(parent)) =
                    located_build_ident_with_component.ident.build()
                {
                    let parent_ident: BuildIdent = match (**parent).clone().try_into() {
                        Ok(ident) => ident,
                        Err(err) => {
                            let msg = self.pool.intern_string(format!(
                                "failed to get valid parent ident for '{}': {err}",
                                located_build_ident_with_component.ident
                            ));
                            return Dependencies::Unknown(msg);
                        }
                    };
                    let parent = match repo.read_package(&parent_ident).await {
                        Ok(spec) => spec,
                        Err(err) => {
                            let msg = self.pool.intern_string(format!(
                                "failed to read parent package for '{}': {err}",
                                located_build_ident_with_component.ident
                            ));
                            return Dependencies::Unknown(msg);
                        }
                    };
                    // Look through the components of the parent to see
                    // if one (or more?) of them embeds this component.
                    let mut found = false;
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
                                    && embedded_package.components().contains(actual_component)
                            })
                            .for_each(|_embedded_package| {
                                found = true;
                                let dep_name = self.pool.intern_package_name(
                                    ResolvoPackageName::PkgNameBufWithComponent(
                                        PkgNameBufWithComponent {
                                            name: parent_ident.name().to_owned(),
                                            component: SyntheticComponent::Actual(
                                                parent_component.name.clone(),
                                            ),
                                        },
                                    ),
                                );
                                let options_for_parent = PkgRequestOptions::from_iter(
                                    package.runtime_requirements().iter().filter_map(|req| {
                                        let var = req.var_ref()?;
                                        (var.var.namespace() == Some(parent.name())).then(|| {
                                            (
                                                var.var.clone(),
                                                PkgRequestOptionValue::Complete(
                                                    var.value.to_string(),
                                                ),
                                            )
                                        })
                                    }),
                                );
                                known_deps.requirements.push(
                                    self.pool
                                        .intern_version_set(
                                            dep_name,
                                            RequestVS::SpkRequest(RequestWithOptions::Pkg(
                                                PkgRequestWithOptions {
                                                    options: options_for_parent,
                                                    pkg_request: PkgRequest::new(
                                                        RangeIdent {
                                                            repository_name: Some(
                                                                located_build_ident_with_component
                                                                    .ident
                                                                    .repository_name()
                                                                    .to_owned(),
                                                            ),
                                                            name: parent_ident.name().to_owned(),
                                                            components: BTreeSet::from_iter([
                                                                parent_component.name.clone(),
                                                            ]),
                                                            version: VersionFilter::single(
                                                                DoubleEqualsVersion::version_range(
                                                                    parent_ident.version().clone(),
                                                                ),
                                                            ),
                                                            build: Some(
                                                                parent_ident.build().clone(),
                                                            ),
                                                        },
                                                        RequestedBy::Embedded(
                                                            located_build_ident_with_component
                                                                .ident
                                                                .target()
                                                                .clone(),
                                                        ),
                                                    ),
                                                },
                                            )),
                                        )
                                        .into(),
                                );
                            });
                    }
                    if !found {
                        // In the event that no owning component was found,
                        // this stub must still bring in at least one
                        // component from the parent. By convention, bring
                        // in the Run component of the parent.
                        let dep_name = self.pool.intern_package_name(
                            ResolvoPackageName::PkgNameBufWithComponent(PkgNameBufWithComponent {
                                name: parent_ident.name().to_owned(),
                                component: SyntheticComponent::Actual(Component::Run),
                            }),
                        );
                        let located_parent = LocatedBuildIdentWithComponent {
                            ident: parent_ident.clone().to_located(
                                located_build_ident_with_component
                                    .ident
                                    .repository_name()
                                    .to_owned(),
                            ),
                            // as_request_with_components does not make use
                            // of the component field, assigning Base here
                            // does not imply anything.
                            component: SyntheticComponent::Base,
                            requires_build_from_source: false,
                        };
                        known_deps.requirements.push(
                            self.pool
                                .intern_version_set(
                                    dep_name,
                                    RequestVS::SpkRequest(
                                        located_parent
                                            .as_request_with_components(&package, [Component::Run]),
                                    ),
                                )
                                .into(),
                        );
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
                    if self
                        .known_global_var_values
                        .borrow_mut()
                        .entry(var_opt.var.without_namespace().to_owned())
                        .or_default()
                        .insert(VarValue::Owned(value.clone()))
                        && self
                            .queried_global_var_values
                            .borrow()
                            .contains(var_opt.var.without_namespace())
                    {
                        // Seeing a new value for a var that has already locked
                        // in the list of candidates.
                        *self.cancel_solving.borrow_mut() = Some(format!(
                            "Saw new value for global var: {}/{value}",
                            var_opt.var.without_namespace()
                        ));
                    }
                    let dep_name = self.pool.intern_package_name(ResolvoPackageName::GlobalVar(
                        var_opt.var.without_namespace().to_owned(),
                    ));
                    // Add a constraint not a dependency because the package
                    // is targeting a specific global var value but there may
                    // not be a request for that var of a specific value.
                    known_deps.constrains.push(self.pool.intern_version_set(
                        dep_name,
                        RequestVS::GlobalVar {
                            key: var_opt.var.without_namespace().to_owned(),
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
        if let Some(msg) = self.cancel_solving.borrow().as_ref() {
            // Eventually there will be more than one reason the solve is
            // cancelled...
            Some(Box::new(msg.clone()))
        } else {
            None
        }
    }
}

impl Interner for SpkProvider {
    fn display_solvable(&self, solvable: SolvableId) -> impl std::fmt::Display {
        let solvable = self.pool.resolve_solvable(solvable);
        format!("{}", solvable.record)
    }

    fn display_name(&self, name: NameId) -> impl std::fmt::Display {
        self.pool.resolve_package_name(name)
    }

    fn display_version_set(&self, version_set: VersionSetId) -> impl std::fmt::Display {
        self.pool.resolve_version_set(version_set)
    }

    fn display_string(&self, string_id: StringId) -> impl std::fmt::Display {
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
