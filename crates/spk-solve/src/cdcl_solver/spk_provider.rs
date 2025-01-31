// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::cell::RefCell;
use std::collections::HashMap;
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
use spk_schema::ident::LocatedBuildIdent;
use spk_schema::name::PkgNameBuf;
use spk_schema::{Package, Request, VersionIdent};
use spk_storage::RepositoryHandle;

use super::pkg_request_version_set::PkgRequestVS;

pub(crate) struct SpkProvider {
    pub(crate) pool: Pool<PkgRequestVS, PkgNameBuf>,
    repos: Vec<Arc<RepositoryHandle>>,
    interned_solvables: RefCell<HashMap<LocatedBuildIdent, SolvableId>>,
}

impl SpkProvider {
    pub fn new(repos: Vec<Arc<RepositoryHandle>>) -> Self {
        Self {
            pool: Pool::new(),
            repos,
            interned_solvables: Default::default(),
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
                    .intern_version_set(dep_name, PkgRequestVS(req.clone()))
                    .into()
            })
            .collect()
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
        let pkg_request_vs = self.pool.resolve_version_set(version_set);
        for candidate in candidates {
            let solvable = self.pool.resolve_solvable(*candidate);
            let located_build_ident = &solvable.record;
            let compatible = pkg_request_vs
                .0
                .is_version_applicable(located_build_ident.version());
            if compatible.is_ok() {
                // XXX: This find runtime will add up.
                let repo = self
                    .repos
                    .iter()
                    .find(|repo| repo.name() == located_build_ident.repository_name())
                    .expect(
                        "Expected solved package's repository to be in the list of repositories",
                    );
                if let Ok(package) = repo.read_package(located_build_ident.target()).await {
                    if pkg_request_vs.0.is_satisfied_by(&package).is_ok() ^ inverse {
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
        selected
    }

    async fn get_candidates(&self, name: NameId) -> Option<Candidates> {
        let pkg_name = self.pool.resolve_package_name(name);

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

                located_builds.extend(
                    builds
                        .into_iter()
                        .map(|build| LocatedBuildIdent::new(repo.name().to_owned(), build)),
                );
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
                .entry(build.clone())
                .or_insert_with(|| self.pool.intern_solvable(name, build));
            candidates.candidates.push(solvable_id);
        }

        Some(candidates)
    }

    async fn sort_candidates(&self, _solver: &SolverCache<Self>, solvables: &mut [SolvableId]) {
        // This implementation just picks the highest version.
        solvables.sort_by(|a, b| {
            let a = self.pool.resolve_solvable(*a);
            let b = self.pool.resolve_solvable(*b);
            b.record.version().cmp(a.record.version())
        });
    }

    async fn get_dependencies(&self, solvable: SolvableId) -> Dependencies {
        // TODO: get dependencies!
        let solvable = self.pool.resolve_solvable(solvable);
        let located_build_ident = &solvable.record;
        // XXX: This find runtime will add up.
        let repo = self
            .repos
            .iter()
            .find(|repo| repo.name() == located_build_ident.repository_name())
            .expect("Expected solved package's repository to be in the list of repositories");
        match repo.read_package(located_build_ident.target()).await {
            Ok(package) => {
                let mut known_deps = KnownDependencies {
                    requirements: Vec::with_capacity(package.runtime_requirements().len()),
                    // This is where IfAlreadyPresent constraints would go.
                    constrains: Vec::new(),
                };
                for requirement in package.runtime_requirements().iter() {
                    // TODO: var requests?
                    let Request::Pkg(req) = requirement else {
                        continue;
                    };
                    let dep_name = self.pool.intern_package_name(req.pkg.name().to_owned());
                    known_deps.requirements.push(
                        self.pool
                            .intern_version_set(dep_name, PkgRequestVS(req.clone()))
                            .into(),
                    );
                }
                Dependencies::Known(known_deps)
            }
            Err(err) => {
                let msg = self.pool.intern_string(err.to_string());
                Dependencies::Unknown(msg)
            }
        }
    }
}

impl Interner for SpkProvider {
    fn display_solvable(&self, _solvable: SolvableId) -> impl std::fmt::Display + '_ {
        "todo: display_solvable"
    }

    fn display_name(&self, _name: NameId) -> impl std::fmt::Display + '_ {
        "todo: display_name"
    }

    fn display_version_set(&self, _version_set: VersionSetId) -> impl std::fmt::Display + '_ {
        "todo: display_version_set"
    }

    fn display_string(&self, _string_id: StringId) -> impl std::fmt::Display + '_ {
        "todo: display_string"
    }

    fn version_set_name(&self, version_set: VersionSetId) -> NameId {
        self.pool.resolve_version_set_package_name(version_set)
    }

    fn solvable_name(&self, solvable: SolvableId) -> NameId {
        self.pool.resolve_solvable(solvable).name
    }

    fn version_sets_in_union(
        &self,
        _version_set_union: VersionSetUnionId,
    ) -> impl Iterator<Item = VersionSetId> {
        // TODO
        Vec::new().into_iter()
    }
}
