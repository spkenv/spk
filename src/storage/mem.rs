// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::Repository;
use crate::api::PkgName;
use crate::{api, Error, Result};

type ComponentMap = HashMap<api::Component, spfs::encoding::Digest>;
type BuildMap = HashMap<api::Build, (api::Spec, ComponentMap)>;

#[derive(Default, Clone, Debug)]
pub struct MemRepository {
    specs: Arc<RwLock<HashMap<PkgName, HashMap<api::Version, api::Spec>>>>,
    packages: Arc<RwLock<HashMap<PkgName, HashMap<api::Version, BuildMap>>>>,
}

impl std::hash::Hash for MemRepository {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self as *const _ as usize).hash(state)
    }
}

impl PartialEq for MemRepository {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for MemRepository {}

impl Repository for MemRepository {
    fn address(&self) -> url::Url {
        let address = format!("mem://{:x}", self as *const _ as usize);
        url::Url::parse(&address)
            .expect("[INTERNAL ERROR] hex address should always create a valid url")
    }

    fn list_packages(&self) -> Result<Vec<api::PkgName>> {
        Ok(self
            .specs
            .read()
            .unwrap()
            .keys()
            .map(|s| s.to_owned())
            .collect())
    }

    fn list_package_versions(&self, name: &api::PkgName) -> Result<Vec<api::Version>> {
        if let Some(specs) = self.specs.read().unwrap().get(name) {
            Ok(specs.keys().map(|v| v.to_owned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        if let Some(versions) = self.packages.read().unwrap().get(&pkg.name) {
            if let Some(builds) = versions.get(&pkg.version) {
                Ok(builds
                    .keys()
                    .map(|b| pkg.with_build(Some(b.clone())))
                    .collect())
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    fn list_build_components(&self, pkg: &api::Ident) -> Result<Vec<api::Component>> {
        let build = match pkg.build.as_ref() {
            Some(b) => b,
            None => return Ok(Vec::new()),
        };
        Ok(self
            .packages
            .read()
            .unwrap()
            .get(&pkg.name)
            .and_then(|versions| versions.get(&pkg.version))
            .and_then(|builds| builds.get(build))
            .map(|(_, build_map)| build_map)
            .map(|cmpts| cmpts.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default())
    }

    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec> {
        match &pkg.build {
            None => self
                .specs
                .read()
                .unwrap()
                .get(&pkg.name)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(&pkg.version)
                .map(|s| s.to_owned())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone())),
            Some(build) => self
                .packages
                .read()
                .unwrap()
                .get(&pkg.name)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(&pkg.version)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(build)
                .map(|(b, _)| b.to_owned())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone())),
        }
    }

    fn get_package(&self, pkg: &api::Ident) -> Result<ComponentMap> {
        match &pkg.build {
            None => Err(Error::PackageNotFoundError(pkg.clone())),
            Some(build) => self
                .packages
                .read()
                .unwrap()
                .get(&pkg.name)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(&pkg.version)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(build)
                .map(|(_, d)| d.to_owned())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone())),
        }
    }

    fn publish_spec(&self, spec: api::Spec) -> Result<()> {
        if spec.pkg.build.is_some() {
            return Err(Error::String(format!(
                "Spec must be published with no build, got {}",
                spec.pkg
            )));
        }
        let mut specs = self.specs.write().unwrap();
        let versions = specs.entry(spec.pkg.name.clone()).or_default();
        if versions.contains_key(&spec.pkg.version) {
            Err(Error::VersionExistsError(spec.pkg))
        } else {
            versions.insert(spec.pkg.version.clone(), spec);
            Ok(())
        }
    }

    fn remove_spec(&self, pkg: &api::Ident) -> Result<()> {
        let mut specs = self.specs.write().unwrap();
        let versions = match specs.get_mut(&pkg.name) {
            Some(v) => v,
            None => return Err(Error::PackageNotFoundError(pkg.clone())),
        };
        if versions.remove(&pkg.version).is_none() {
            Err(Error::PackageNotFoundError(pkg.clone()))
        } else {
            Ok(())
        }
    }

    fn force_publish_spec(&self, spec: api::Spec) -> Result<()> {
        if let Some(api::Build::Embedded) = spec.pkg.build {
            return Err(api::InvalidBuildError::new_error(
                "Cannot publish embedded package".to_string(),
            ));
        }

        // The spec could be for a build or a version. They are
        // handled differently because of where this repo stores each
        // kind of spec.
        match &spec.pkg.build {
            Some(b) => {
                // A build spec, e.g. package/version/build. This will
                // overwrite the build spec, but keep the build's
                // current components, if any.
                let mut packages = self.packages.write().unwrap();
                let versions = packages.entry(spec.pkg.name.clone()).or_default();
                let builds = versions.entry(spec.pkg.version.clone()).or_default();
                let components = match builds.get(b) {
                    Some(t) => t.1.clone(),
                    None => ComponentMap::default(),
                };
                drop(packages); // this lock will be needed to publish
                self.publish_package(spec, components)
            }
            None => {
                // A version spec e.g. package/version. This will remove
                // the existing version spec and use publish_spec to add
                // the new one. It does not change the build specs, which
                // are stored in the packages field
                let mut specs = self.specs.write().unwrap();
                let versions = specs.entry(spec.pkg.name.clone()).or_default();
                versions.remove(&spec.pkg.version);
                drop(specs); // this lock will be needed to publish
                self.publish_spec(spec)
            }
        }
    }

    fn publish_package(&self, spec: api::Spec, components: ComponentMap) -> Result<()> {
        let build = match &spec.pkg.build {
            Some(b) => b.to_owned(),
            None => {
                return Err(Error::String(format!(
                    "Package must include a build in order to be published: {}",
                    spec.pkg
                )))
            }
        };

        let mut packages = self.packages.write().unwrap();
        let versions = packages.entry(spec.pkg.name.clone()).or_default();
        let builds = versions.entry(spec.pkg.version.clone()).or_default();

        builds.insert(build, (spec, components));
        Ok(())
    }

    fn remove_package(&self, pkg: &api::Ident) -> Result<()> {
        let build = match &pkg.build {
            Some(b) => b,
            None => {
                return Err(Error::String(format!(
                    "Package must include a build in order to be removed: {}",
                    pkg
                )))
            }
        };

        let mut packages = self.packages.write().unwrap();
        let versions = match packages.get_mut(&pkg.name) {
            Some(v) => v,
            None => return Err(Error::PackageNotFoundError(pkg.clone())),
        };

        let builds = match versions.get_mut(&pkg.version) {
            Some(v) => v,
            None => return Err(Error::PackageNotFoundError(pkg.clone())),
        };

        if builds.remove(build).is_none() {
            Err(Error::PackageNotFoundError(pkg.clone()))
        } else {
            Ok(())
        }
    }
}
