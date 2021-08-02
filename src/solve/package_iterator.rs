// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use crate::api;
use dyn_clone::DynClone;
use pyo3::{prelude::*, types::PyTuple};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use super::errors::PackageNotFoundError;
use super::solution::PackageSource;

pub trait BuildIterator: DynClone + Send + Sync {
    fn is_empty(&self) -> bool;
    fn next(&mut self) -> crate::Result<Option<(api::Spec, PackageSource)>>;
}

dyn_clone::clone_trait_object!(BuildIterator);

type PackageIteratorItem = (api::Ident, Arc<Mutex<dyn BuildIterator>>);

pub trait PackageIterator: DynClone + Send + Sync {
    fn next(&mut self) -> crate::Result<Option<PackageIteratorItem>>;

    /// Replaces the internal build iterator for version with the given one.
    fn set_builds(&mut self, version: &api::Version, builds: Arc<Mutex<dyn BuildIterator>>);
}

dyn_clone::clone_trait_object!(PackageIterator);

#[derive(Clone)]
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
pub struct RepositoryPackageIterator {
    pub package_name: String,
    pub repos: Vec<PyObject>,
    versions: Option<VersionIterator>,
    version_map: HashMap<api::Version, PyObject>,
    builds_map: HashMap<api::Version, Arc<Mutex<dyn BuildIterator>>>,
    active_version: Option<api::Version>,
}

impl Clone for RepositoryPackageIterator {
    /// Create a copy of this iterator, with the cursor at the same point.
    fn clone(&self) -> Self {
        let version_map = if self.versions.is_none() {
            match self.build_version_map() {
                Ok(version_map) => version_map,
                // XXX: This only caught PackageNotFoundError in the python impl
                Err(_) => {
                    return RepositoryPackageIterator::new(
                        self.package_name.to_owned(),
                        self.repos.clone(),
                    )
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

    fn set_builds(&mut self, _version: &api::Version, _builds: Arc<Mutex<dyn BuildIterator>>) {
        todo!()
    }
}

impl RepositoryPackageIterator {
    pub fn new(package_name: String, repos: Vec<PyObject>) -> Self {
        RepositoryPackageIterator {
            package_name,
            repos,
            versions: None,
            version_map: HashMap::default(),
            builds_map: HashMap::default(),
            active_version: None,
        }
    }

    fn build_version_map(&self) -> PyResult<HashMap<api::Version, PyObject>> {
        let mut version_map = HashMap::default();
        for repo in self.repos.iter().rev() {
            let repo_versions: PyResult<Vec<String>> = Python::with_gil(|py| {
                let args = PyTuple::new(py, &[self.package_name.as_str()]);
                let iter = repo.call_method1(py, "list_package_versions", args)?;
                iter.as_ref(py)
                    .iter()?
                    .map(|o| o.and_then(PyAny::extract::<String>))
                    .collect()
            });
            for version_str in repo_versions? {
                let version = api::parse_version(version_str)?;
                version_map.insert(version, repo.clone());
            }
        }

        if version_map.is_empty() {
            return Err(PackageNotFoundError::new_err(self.package_name.to_owned()));
        }

        Ok(version_map)
    }

    fn start(&mut self) -> crate::Result<()> {
        self.version_map = self.build_version_map()?;
        let mut versions: Vec<api::Version> =
            self.version_map.keys().into_iter().cloned().collect();
        versions.sort();
        versions.reverse();
        self.versions = Some(VersionIterator::new(versions.into()));
        Ok(())
    }
}

#[derive(Clone)]
pub struct RepositoryBuildIterator {
    pkg: api::Ident,
    repo: PyObject,
    builds: VecDeque<api::Ident>,
    spec: Option<api::Spec>,
}

impl BuildIterator for RepositoryBuildIterator {
    fn is_empty(&self) -> bool {
        self.builds.is_empty()
    }

    fn next(&mut self) -> crate::Result<Option<(api::Spec, PackageSource)>> {
        todo!()
    }
}

impl RepositoryBuildIterator {
    fn new(pkg: api::Ident, repo: PyObject) -> PyResult<Self> {
        let (builds, spec) = Python::with_gil(|py| {
            // XXX: Ident: ToPyObject missing?
            let args = PyTuple::new(py, &[pkg.to_string()]);
            let iter = repo.call_method1(py, "list_package_builds", args)?;
            let builds: PyResult<Vec<api::Ident>> = iter
                .as_ref(py)
                .iter()?
                .map(|o| o.and_then(PyAny::extract::<api::Ident>))
                .collect();
            let builds = builds?;

            let spec = match repo.call_method1(py, "read_spec", args) {
                Ok(spec) => Some(spec.as_ref(py).extract::<api::Spec>()?),
                // FIXME: This should only catch PackageNotFoundError
                Err(_) => None,
            };

            PyResult::Ok((builds, spec))
        })?;
        Ok(RepositoryBuildIterator {
            pkg,
            repo,
            builds: builds.into(),
            spec,
        })
    }
}

#[derive(Clone)]
pub struct EmptyBuildIterator {}

impl BuildIterator for EmptyBuildIterator {
    fn is_empty(&self) -> bool {
        true
    }

    fn next(&mut self) -> crate::Result<Option<(api::Spec, PackageSource)>> {
        Ok(None)
    }
}

impl EmptyBuildIterator {
    pub fn new() -> Self {
        EmptyBuildIterator {}
    }
}

#[derive(Clone)]
pub struct SortedBuildIterator {
    options: api::OptionMap,
    source: Box<dyn BuildIterator>,
}

impl BuildIterator for SortedBuildIterator {
    fn is_empty(&self) -> bool {
        todo!()
    }

    fn next(&mut self) -> crate::Result<Option<(api::Spec, PackageSource)>> {
        todo!()
    }
}

impl SortedBuildIterator {
    pub fn new(_options: api::OptionMap, _source: Arc<Mutex<dyn BuildIterator>>) -> Self {
        todo!()
    }
}
