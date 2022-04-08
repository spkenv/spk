// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use dyn_clone::DynClone;
use pyo3::prelude::*;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use super::solution::PackageSource;
use crate::{
    api::{self, Build},
    build::BuildVariant,
    storage, Error, Result,
};

pub trait BuildIterator: DynClone + Send + Sync + std::fmt::Debug {
    fn is_empty(&self) -> bool;
    fn is_sorted_build_iterator(&self) -> bool {
        false
    }
    fn next(&mut self) -> crate::Result<Option<(Arc<api::SpecWithBuildVariant>, PackageSource)>>;
    /// Return the non-build-specific Spec
    fn version_spec(&self) -> Option<Arc<api::Spec>>;
}

dyn_clone::clone_trait_object!(BuildIterator);

type PackageIteratorItem = (api::Ident, Arc<Mutex<dyn BuildIterator>>);

pub trait PackageIterator: DynClone + Send + Sync + std::fmt::Debug {
    fn next(&mut self) -> crate::Result<Option<PackageIteratorItem>>;

    /// Replaces the internal build iterator for version with the given one.
    fn set_builds(&mut self, version: &api::Version, builds: Arc<Mutex<dyn BuildIterator>>);
}

dyn_clone::clone_trait_object!(PackageIterator);

#[derive(Clone, Debug)]
struct VersionIterator {
    versions: VecDeque<api::Version>,
}

impl Iterator for VersionIterator {
    type Item = api::Version;

    fn next(&mut self) -> Option<Self::Item> {
        self.versions.pop_front()
    }
}

impl VersionIterator {
    fn new(versions: VecDeque<api::Version>) -> Self {
        VersionIterator { versions }
    }
}

/// A stateful cursor yielding package builds from a set of repositories.
#[derive(Debug)]
pub struct RepositoryPackageIterator {
    pub package_name: String,
    pub repos: Vec<Arc<storage::RepositoryHandle>>,
    versions: Option<VersionIterator>,
    version_map: HashMap<api::Version, Arc<storage::RepositoryHandle>>,
    builds_map: HashMap<api::Version, Arc<Mutex<dyn BuildIterator>>>,
    active_version: Option<api::Version>,
}

impl Clone for RepositoryPackageIterator {
    /// Create a copy of this iterator, with the cursor at the same point.
    fn clone(&self) -> Self {
        let version_map = if self.versions.is_none() {
            match self.build_version_map() {
                Ok(version_map) => version_map,
                Err(Error::PackageNotFoundError(_)) => {
                    return RepositoryPackageIterator::new(
                        self.package_name.to_owned(),
                        self.repos.clone(),
                    )
                }
                Err(err) => {
                    // we wanted to save the clone from causing this
                    // work to be done twice, but it's not fatal
                    tracing::trace!(
                        "Encountered error cloning RepositoryPackageIterator: {:?}",
                        err
                    );
                    self.version_map.clone()
                }
            }
        } else {
            self.version_map.clone()
        };

        RepositoryPackageIterator {
            package_name: self.package_name.clone(),
            repos: self.repos.clone(),
            versions: self.versions.clone(),
            version_map,
            // Python custom clone() doesn't clone the remaining fields
            builds_map: HashMap::default(),
            active_version: None,
        }
    }
}

impl PackageIterator for RepositoryPackageIterator {
    fn next(&mut self) -> crate::Result<Option<PackageIteratorItem>> {
        if self.versions.is_none() {
            self.start()?
        }

        if self.active_version.is_none() {
            self.active_version = self.versions.as_mut().and_then(|i| i.next());
        }
        let version = if let Some(active_version) = self.active_version.as_ref() {
            active_version
        } else {
            return Ok(None);
        };
        let repo = if let Some(repo) = self.version_map.get(version) {
            repo
        } else {
            return Err(crate::Error::String(
                "version not found in version_map".to_owned(),
            ));
        };
        let pkg = api::Ident::newpy(self.package_name.as_str(), Some(version.clone()), None)?;
        if !self.builds_map.contains_key(version) {
            self.builds_map.insert(
                version.clone(),
                Arc::new(Mutex::new(RepositoryBuildIterator::new(
                    pkg.clone(),
                    repo.clone(),
                )?)),
            );
        }
        let builds = self.builds_map.get(version).unwrap();
        if builds.lock().unwrap().is_empty() {
            self.active_version = None;
            return self.next();
        }
        Ok(Some((pkg, builds.clone())))
    }

    fn set_builds(&mut self, version: &api::Version, builds: Arc<Mutex<dyn BuildIterator>>) {
        self.builds_map.insert(version.clone(), builds);
    }
}

impl RepositoryPackageIterator {
    pub fn new(package_name: String, repos: Vec<Arc<storage::RepositoryHandle>>) -> Self {
        RepositoryPackageIterator {
            package_name,
            repos,
            versions: None,
            version_map: HashMap::default(),
            builds_map: HashMap::default(),
            active_version: None,
        }
    }

    fn build_version_map(&self) -> Result<HashMap<api::Version, Arc<storage::RepositoryHandle>>> {
        let mut version_map = HashMap::default();
        for repo in self.repos.iter().rev() {
            for version in repo.list_package_versions(&self.package_name)? {
                version_map.insert(version, repo.clone());
            }
        }

        if version_map.is_empty() {
            return Err(Error::PackageNotFoundError(api::Ident::new(
                &self.package_name,
            )?));
        }

        Ok(version_map)
    }

    fn start(&mut self) -> Result<()> {
        self.version_map = self.build_version_map()?;
        let mut versions: Vec<api::Version> =
            self.version_map.keys().into_iter().cloned().collect();
        versions.sort();
        versions.reverse();
        self.versions = Some(VersionIterator::new(versions.into()));
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RepositoryBuildIterator {
    repo: Arc<storage::RepositoryHandle>,
    builds: VecDeque<api::BuildIdent>,
    spec: Option<Arc<api::Spec>>,
}

impl BuildIterator for RepositoryBuildIterator {
    fn is_empty(&self) -> bool {
        self.builds.is_empty()
    }

    fn next(&mut self) -> crate::Result<Option<(Arc<api::SpecWithBuildVariant>, PackageSource)>> {
        let build = if let Some(build) = self.builds.pop_front() {
            build
        } else {
            return Ok(None);
        };

        let mut spec = match self.repo.read_build_spec(&build) {
            Ok(spec) => spec,
            Err(Error::PackageNotFoundError(..)) => {
                tracing::warn!(
                    "Repository listed build with no spec: {} from {:?}",
                    build,
                    self.repo
                );
                return self.next();
            }
            Err(err) => return Err(err),
        };

        let components = match self.repo.get_package(&(&build).into()) {
            Ok(c) => c,
            Err(Error::PackageNotFoundError(..)) => Default::default(),
            Err(err) => return Err(err),
        };

        if spec.pkg.build.is_none() {
            tracing::warn!(
                "Published spec is corrupt (has no associated build), pkg={}",
                build,
            );
            let mut new_spec = (*spec.spec).clone();
            new_spec.pkg = new_spec.pkg.with_build(Some(build.build));
            spec.spec = Arc::new(new_spec);
        }

        Ok(Some((
            Arc::new(spec),
            PackageSource::Repository {
                repo: self.repo.clone(),
                components,
            },
        )))
    }

    fn version_spec(&self) -> Option<Arc<api::Spec>> {
        self.spec.clone()
    }
}

impl RepositoryBuildIterator {
    fn new(pkg: api::Ident, repo: Arc<storage::RepositoryHandle>) -> Result<Self> {
        let mut builds = repo.list_package_builds(&pkg)?;
        let spec = match repo.read_spec(&pkg) {
            Ok(spec) => Some(Arc::new(spec)),
            Err(Error::PackageNotFoundError(..)) => None,
            Err(err) => return Err(err),
        };

        // source packages must come last to ensure that building
        // from source is the last option under normal circumstances
        builds.sort_by_key(|pkg| !pkg.is_source());

        Ok(RepositoryBuildIterator {
            repo,
            builds: builds.into(),
            spec,
        })
    }
}

#[derive(Clone, Debug)]
pub struct EmptyBuildIterator {}

impl BuildIterator for EmptyBuildIterator {
    fn is_empty(&self) -> bool {
        true
    }

    fn next(&mut self) -> crate::Result<Option<(Arc<api::SpecWithBuildVariant>, PackageSource)>> {
        Ok(None)
    }

    fn version_spec(&self) -> Option<Arc<api::Spec>> {
        None
    }
}

impl EmptyBuildIterator {
    pub fn new() -> Self {
        EmptyBuildIterator {}
    }
}

#[derive(Clone, Debug)]
pub struct SortedBuildIterator {
    options: api::OptionMap,
    source: Arc<Mutex<dyn BuildIterator>>,
    builds: VecDeque<(Arc<api::SpecWithBuildVariant>, PackageSource)>,
}

impl BuildIterator for SortedBuildIterator {
    fn is_empty(&self) -> bool {
        self.builds.is_empty()
    }

    fn is_sorted_build_iterator(&self) -> bool {
        true
    }

    fn next(&mut self) -> crate::Result<Option<(Arc<api::SpecWithBuildVariant>, PackageSource)>> {
        Ok(self.builds.pop_front())
    }

    fn version_spec(&self) -> Option<Arc<api::Spec>> {
        self.source.lock().unwrap().version_spec()
    }
}

impl SortedBuildIterator {
    pub fn new(options: api::OptionMap, source: Arc<Mutex<dyn BuildIterator>>) -> PyResult<Self> {
        let mut builds = VecDeque::new();
        {
            let mut source_lock = source.lock().unwrap();
            while let Some(item) = source_lock.next()? {
                builds.push_back(item);
            }
        }

        let mut sbi = SortedBuildIterator {
            options,
            source,
            builds,
        };

        sbi.sort();

        Ok(sbi)
    }

    #[allow(clippy::nonminimal_bool)]
    fn sort(&mut self) {
        let version_spec = self.version_spec();
        let variant_count = version_spec
            .as_ref()
            .map(|s| s.build.variants.len())
            .unwrap_or(0);
        let default_options = version_spec
            .as_ref()
            .map(|s| {
                // XXX: Passing Default here for now, this sort routine
                // logic is getting an overhaul in a separate branch
                s.resolve_all_options(&BuildVariant::Default, &api::OptionMap::default())
                    .unwrap()
            })
            .unwrap_or_else(api::OptionMap::default);

        let self_options = self.options.clone();

        self.builds
            .make_contiguous()
            .sort_by_cached_key(|(spec, _)| {
                let build = spec
                    .pkg
                    .build
                    .as_ref()
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "None".to_owned());
                let total_options_count = spec.build.options.len();
                // source packages must come last to ensure that building
                // from source is the last option under normal circumstances
                if spec.pkg.build.is_none() || spec.pkg.build == Some(Build::Source) {
                    return ((variant_count + total_options_count + 1) as i64, build);
                }

                if let Some(version_spec) = &version_spec {
                    // if this spec is compatible with the default options, it's the
                    // most valuable
                    if !!&spec
                        .build
                        .validate_options(spec.pkg.name(), &default_options)
                    {
                        return (-1, build);
                    }
                    // then we sort based on the first defined variant that seems valid
                    for (i, variant) in version_spec.build.variants.iter().enumerate() {
                        if !!&spec.build.validate_options(spec.pkg.name(), variant) {
                            return (i as i64, build);
                        }
                    }
                }

                // and then it's the distance from the default option set,
                // where distance is just the number of differing options
                let current_options_unfiltered = spec
                    .resolve_all_options(&api::OptionMap::default())
                    .unwrap();
                let current_options: HashSet<(&String, &String)> = current_options_unfiltered
                    .iter()
                    .filter(|&(o, _)| self_options.contains_key(o))
                    .collect();
                let similar_options_count = default_options
                    .iter()
                    .collect::<HashSet<_>>()
                    .intersection(&current_options)
                    .collect::<HashSet<_>>()
                    .len();
                let distance_from_default = (total_options_count - similar_options_count).max(0);
                ((variant_count + distance_from_default) as i64, build)
            });
    }
}
