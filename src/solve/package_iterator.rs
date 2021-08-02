// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use crate::api;
use dyn_clone::DynClone;
use pyo3::prelude::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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

trait VersionIter: Iterator<Item = api::Version> + DynClone + Send + Sync {}

dyn_clone::clone_trait_object!(VersionIter);

/// A stateful cursor yielding package builds from a set of repositories.
#[derive(Clone)]
pub struct RepositoryPackageIterator {
    pub package_name: String,
    pub repos: Vec<PyObject>,
    versions: Option<Arc<Mutex<dyn VersionIter>>>,
    version_map: HashMap<api::Version, PyObject>,
    builds_map: HashMap<api::Version, Arc<Mutex<dyn BuildIterator>>>,
    active_version: Option<api::Version>,
}

impl PackageIterator for RepositoryPackageIterator {
    fn next(&mut self) -> crate::Result<Option<PackageIteratorItem>> {
        if self.versions.is_none() {
            self.start()
        }

        if self.active_version.is_none() {
            self.active_version = self
                .versions
                .as_ref()
                .map(|i| i.lock().unwrap().next())
                .flatten();
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
                ))),
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

    fn start(&mut self) {
        todo!()
    }
}

#[derive(Clone)]
pub struct RepositoryBuildIterator {
    pkg: api::Ident,
    repo: PyObject,
}

impl BuildIterator for RepositoryBuildIterator {
    fn is_empty(&self) -> bool {
        todo!()
    }

    fn next(&mut self) -> crate::Result<Option<(api::Spec, PackageSource)>> {
        todo!()
    }
}

impl RepositoryBuildIterator {
    fn new(pkg: api::Ident, repo: PyObject) -> Self {
        RepositoryBuildIterator { pkg, repo }
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
