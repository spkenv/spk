// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
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
use spk_schema::ident::LocatedBuildIdent;
use spk_schema::name::PkgNameBuf;
use spk_schema::{Opt, Package, Request, VersionIdent};
use spk_storage::RepositoryHandle;

use super::pkg_request_version_set::{
    LocatedBuildIdentWithComponent,
    RequestVS,
    SpkSolvable,
    VarValue,
};

// "global-vars--" represents a pseudo- package that accumulates global var
// constraints. The name is intended to never conflict with a real package name,
// however since it is a legal package name that can't be guaranteed.
const PSEUDO_PKG_NAME_PREFIX: &str = "global-vars--";

pub(crate) struct SpkProvider {
    pub(crate) pool: Pool<RequestVS, PkgNameBuf>,
    repos: Vec<Arc<RepositoryHandle>>,
    interned_solvables: RefCell<HashMap<SpkSolvable, SolvableId>>,
    /// Track all the global var keys and values that have been witnessed while
    /// solving.
    known_global_var_values: RefCell<HashMap<String, HashSet<VarValue>>>,
    /// Track which global var candidates have been queried by the solver. Once
    /// queried, it is no longer possible to add more possible values without
    /// restarting the solve.
    queried_global_var_values: RefCell<HashSet<String>>,
    cancel_solving: Cell<bool>,
}

impl SpkProvider {
    pub fn new(repos: Vec<Arc<RepositoryHandle>>) -> Self {
        Self {
            pool: Pool::new(),
            repos,
            interned_solvables: Default::default(),
            known_global_var_values: Default::default(),
            queried_global_var_values: Default::default(),
            cancel_solving: Default::default(),
        }
    }

    pub fn pkg_requirements(&self, requests: &[Request]) -> Vec<Requirement> {
        requests
            .iter()
            .filter_map(|req| match req {
                Request::Pkg(pkg) => Some(pkg),
                _ => None,
            })
            .map(|req| {
                let dep_name = self.pool.intern_package_name(req.pkg.name().to_owned());
                self.pool
                    .intern_version_set(dep_name, RequestVS::SpkRequest(Request::Pkg(req.clone())))
                    .into()
            })
            .collect()
    }

    pub fn is_canceled(&self) -> bool {
        self.cancel_solving.get()
    }

    /// Return a new provider to restart the solve, preserving what was learned
    /// about global variables.
    pub fn reset(&self) -> Self {
        Self {
            pool: Pool::new(),
            repos: self.repos.clone(),
            interned_solvables: Default::default(),
            known_global_var_values: RefCell::new(self.known_global_var_values.take()),
            queried_global_var_values: Default::default(),
            cancel_solving: Default::default(),
        }
    }

    pub fn var_requirements(&self, _requests: &[Request]) -> Vec<VersionSetId> {
        // TODO
        Vec::new()
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

        if let Some(key) = pkg_name.strip_prefix(PSEUDO_PKG_NAME_PREFIX) {
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
                .list_package_versions(pkg_name)
                .await
                .unwrap_or_default();
            for version in versions.iter() {
                // TODO: We need a borrowing version of this to avoid cloning.
                let pkg_version = VersionIdent::new(pkg_name.clone(), (**version).clone());

                let builds = repo
                    .list_package_builds(&pkg_version)
                    .await
                    .unwrap_or_default();

                for build in builds {
                    let components = repo.list_build_components(&build).await.unwrap_or_default();
                    let located_build_ident = LocatedBuildIdent::new(repo.name().to_owned(), build);
                    for component in components {
                        located_builds.push(LocatedBuildIdentWithComponent {
                            ident: located_build_ident.clone(),
                            component: component
                                .try_into()
                                .expect("list_build_components will never return an All component"),
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
            let solvable_id = *self
                .interned_solvables
                .borrow_mut()
                .entry(SpkSolvable::LocatedBuildIdentWithComponent(build.clone()))
                .or_insert_with(|| {
                    self.pool
                        .intern_solvable(name, SpkSolvable::LocatedBuildIdentWithComponent(build))
                });
            candidates.candidates.push(solvable_id);
        }

        Some(candidates)
    }

    async fn sort_candidates(&self, _solver: &SolverCache<Self>, solvables: &mut [SolvableId]) {
        // This implementation just picks the highest version.
        solvables.sort_by(|a, b| {
            let a = self.pool.resolve_solvable(*a);
            let b = self.pool.resolve_solvable(*b);
            match (&a.record, &b.record) {
                (
                    SpkSolvable::LocatedBuildIdentWithComponent(a),
                    SpkSolvable::LocatedBuildIdentWithComponent(b),
                ) => b.ident.version().cmp(a.ident.version()),
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
        // TODO: get dependencies!
        let solvable = self.pool.resolve_solvable(solvable);
        let SpkSolvable::LocatedBuildIdentWithComponent(located_build_ident_with_component) =
            &solvable.record
        else {
            return Dependencies::Known(KnownDependencies::default());
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
                    let dep_name = self
                        .pool
                        .intern_package_name(pkg_name!(&pseudo_pkg_name).to_owned());
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
                    match requirement {
                        Request::Pkg(pkg_request) => {
                            let dep_name = self
                                .pool
                                .intern_package_name(pkg_request.pkg.name().to_owned());
                            known_deps.requirements.push(
                                self.pool
                                    .intern_version_set(
                                        dep_name,
                                        RequestVS::SpkRequest(requirement.clone()),
                                    )
                                    .into(),
                            );
                        }
                        Request::Var(var_request) => {
                            match &var_request.value {
                                spk_schema::ident::PinnableValue::FromBuildEnv => todo!(),
                                spk_schema::ident::PinnableValue::FromBuildEnvIfPresent => todo!(),
                                spk_schema::ident::PinnableValue::Pinned(value) => {
                                    let dep_name = match var_request.var.namespace() {
                                        Some(pkg_name) => {
                                            self.pool.intern_package_name(pkg_name.to_owned())
                                        }
                                        None => {
                                            // Since we will be adding
                                            // constraints for global vars we
                                            // need to add the pseudo-package
                                            // to the dependency list so it
                                            // will influence decisions.
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
                                                // Seeing a new value for a var
                                                // that has already locked in
                                                // the list of candidates.
                                                self.cancel_solving.set(true);
                                            }
                                            let dep_name = self.pool.intern_package_name(
                                                pkg_name!(&pseudo_pkg_name).to_owned(),
                                            );
                                            known_deps.requirements.push(
                                                self.pool
                                                    .intern_version_set(
                                                        dep_name,
                                                        RequestVS::GlobalVar {
                                                            key: var_request
                                                                .var
                                                                .base_name()
                                                                .to_owned(),
                                                            value: VarValue::ArcStr(Arc::clone(
                                                                value,
                                                            )),
                                                        },
                                                    )
                                                    .into(),
                                            );
                                            dep_name
                                        }
                                    };
                                    // If we end up adding pkg_name to the solve,
                                    // it needs to satisfy this var request.
                                    known_deps.constrains.push(self.pool.intern_version_set(
                                        dep_name,
                                        RequestVS::SpkRequest(requirement.clone()),
                                    ));
                                }
                            }
                        }
                    }
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
