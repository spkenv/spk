// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

use super::Repository;
use crate::api::{Named, Package, PkgNameBuf, Versioned};
use crate::{api, Error, Result};

type ComponentMap = HashMap<api::Component, spfs::encoding::Digest>;
type BuildMap = HashMap<api::Build, (api::Spec, ComponentMap)>;
type SpecByVersion = HashMap<api::Version, Arc<api::Spec>>;

#[derive(Clone, Debug)]
pub struct MemRepository {
    address: url::Url,
    name: api::RepositoryNameBuf,
    specs: Arc<tokio::sync::RwLock<HashMap<PkgNameBuf, SpecByVersion>>>,
    packages: Arc<tokio::sync::RwLock<HashMap<PkgNameBuf, HashMap<api::Version, BuildMap>>>>,
}

impl MemRepository {
    pub fn new() -> Self {
        let specs = Arc::default();
        // Using the address of `specs` because `self` doesn't exist yet.
        let address = format!("mem://{:x}", &specs as *const _ as usize);
        let address = url::Url::parse(&address)
            .expect("[INTERNAL ERROR] hex address should always create a valid url");
        Self {
            address,
            name: "mem".try_into().expect("valid repository name"),
            specs,
            packages: Arc::default(),
        }
    }
}

impl Default for MemRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl std::hash::Hash for MemRepository {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self as *const _ as usize).hash(state)
    }
}

impl Ord for MemRepository {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.address.cmp(&other.address)
    }
}

impl PartialOrd for MemRepository {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for MemRepository {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for MemRepository {}

#[async_trait::async_trait]
impl Repository for MemRepository {
    fn address(&self) -> &url::Url {
        &self.address
    }

    async fn list_packages(&self) -> Result<Vec<api::PkgNameBuf>> {
        Ok(self
            .specs
            .read()
            .await
            .keys()
            .map(|s| s.to_owned())
            .collect())
    }

    async fn list_package_versions(
        &self,
        name: &api::PkgName,
    ) -> Result<Arc<Vec<Arc<api::Version>>>> {
        if let Some(specs) = self.specs.read().await.get(name) {
            Ok(Arc::new(
                specs.keys().map(|v| Arc::new(v.to_owned())).collect(),
            ))
        } else {
            Ok(Arc::new(Vec::new()))
        }
    }

    async fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        if let Some(versions) = self.packages.read().await.get(&pkg.name) {
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

    async fn list_build_components(&self, pkg: &api::Ident) -> Result<Vec<api::Component>> {
        let build = match pkg.build.as_ref() {
            Some(b) => b,
            None => return Ok(Vec::new()),
        };
        Ok(self
            .packages
            .read()
            .await
            .get(&pkg.name)
            .and_then(|versions| versions.get(&pkg.version))
            .and_then(|builds| builds.get(build))
            .map(|(_, build_map)| build_map)
            .map(|cmpts| cmpts.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default())
    }

    fn name(&self) -> &api::RepositoryName {
        self.name.as_ref()
    }

    async fn read_spec(&self, pkg: &api::Ident) -> Result<Arc<api::Spec>> {
        match &pkg.build {
            None => self
                .specs
                .read()
                .await
                .get(&pkg.name)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(&pkg.version)
                .map(Arc::clone)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone())),
            Some(build) => self
                .packages
                .read()
                .await
                .get(&pkg.name)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(&pkg.version)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(build)
                .map(|(b, _)| Arc::new(b.to_owned()))
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone())),
        }
    }

    async fn get_package(&self, pkg: &api::Ident) -> Result<ComponentMap> {
        match &pkg.build {
            None => Err(Error::PackageNotFoundError(pkg.clone())),
            Some(build) => self
                .packages
                .read()
                .await
                .get(&pkg.name)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(&pkg.version)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(build)
                .map(|(_, d)| d.to_owned())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone())),
        }
    }

    async fn publish_spec(&self, spec: &api::Spec) -> Result<()> {
        if spec.ident().build.is_some() {
            return Err(Error::String(format!(
                "Spec must be published with no build, got {}",
                spec.ident()
            )));
        }
        let mut specs = self.specs.write().await;
        let versions = specs.entry(spec.name().to_owned()).or_default();
        if versions.contains_key(spec.version()) {
            Err(Error::VersionExistsError(spec.ident().clone()))
        } else {
            versions.insert(spec.version().clone(), Arc::new(spec.clone()));
            Ok(())
        }
    }

    async fn remove_spec(&self, pkg: &api::Ident) -> Result<()> {
        let mut specs = self.specs.write().await;
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

    async fn force_publish_spec(&self, spec: &api::Spec) -> Result<()> {
        if let Some(api::Build::Embedded) = spec.ident().build {
            return Err(api::InvalidBuildError::new_error(
                "Cannot publish embedded package".to_string(),
            ));
        }

        // The spec could be for a build or a version. They are
        // handled differently because of where this repo stores each
        // kind of spec.
        match &spec.ident().build {
            Some(b) => {
                // A build spec, e.g. package/version/build. This will
                // overwrite the build spec, but keep the build's
                // current components, if any.
                let mut packages = self.packages.write().await;
                let versions = packages.entry(spec.name().to_owned()).or_default();
                let builds = versions.entry(spec.version().clone()).or_default();
                let components = match builds.get(b) {
                    Some(t) => t.1.clone(),
                    None => ComponentMap::default(),
                };
                drop(packages); // this lock will be needed to publish
                self.publish_package(spec, components).await
            }
            None => {
                // A version spec e.g. package/version. This will remove
                // the existing version spec and use publish_spec to add
                // the new one. It does not change the build specs, which
                // are stored in the packages field
                let mut specs = self.specs.write().await;
                let versions = specs.entry(spec.name().to_owned()).or_default();
                versions.remove(spec.version());
                drop(specs); // this lock will be needed to publish
                self.publish_spec(spec).await
            }
        }
    }

    async fn publish_package(&self, spec: &api::Spec, components: ComponentMap) -> Result<()> {
        let build = match &spec.ident().build {
            Some(b) => b.to_owned(),
            None => {
                return Err(Error::String(format!(
                    "Package must include a build in order to be published: {}",
                    spec.ident()
                )))
            }
        };

        let mut packages = self.packages.write().await;
        let versions = packages.entry(spec.name().to_owned()).or_default();
        let builds = versions.entry(spec.version().clone()).or_default();

        builds.insert(build, (spec.clone(), components));
        Ok(())
    }

    async fn remove_package(&self, pkg: &api::Ident) -> Result<()> {
        let build = match &pkg.build {
            Some(b) => b,
            None => {
                return Err(Error::String(format!(
                    "Package must include a build in order to be removed: {}",
                    pkg
                )))
            }
        };

        let mut packages = self.packages.write().await;
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
