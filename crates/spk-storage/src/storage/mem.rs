// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

use spk_ident::Ident;
use spk_ident_build::Build;
use spk_ident_component::Component;
use spk_name::{PkgName, PkgNameBuf, RepositoryName, RepositoryNameBuf};
use spk_spec::SpecRecipe;
use spk_spec_ops::{Named, PackageOps, Versioned};
use spk_version::Version;
use tokio::sync::RwLock;

use super::Repository;
use crate::{Error, Result};

type ComponentMap = HashMap<Component, spfs::encoding::Digest>;
type PackageMap<T> = HashMap<PkgNameBuf, VersionMap<T>>;
type VersionMap<T> = HashMap<Version, T>;
type BuildMap<Package> = HashMap<Build, (Arc<Package>, ComponentMap)>;

#[derive(Clone, Debug)]
pub struct MemRepository<Recipe = SpecRecipe>
where
    Recipe: spk_spec::Recipe + Sync + Send,
    Recipe::Output: Sync + Send,
{
    address: url::Url,
    name: RepositoryNameBuf,
    specs: Arc<RwLock<PackageMap<Arc<Recipe>>>>,
    packages: Arc<RwLock<PackageMap<BuildMap<Recipe::Output>>>>,
}

impl<Recipe, Package> MemRepository<Recipe>
where
    Recipe: spk_spec::Recipe<Output = Package> + Send + Sync,
    Package: spk_spec::Package + Send + Sync,
{
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

impl<Recipe, Package> Default for MemRepository<Recipe>
where
    Recipe: spk_spec::Recipe<Output = Package> + Send + Sync,
    Package: spk_spec::Package + Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Recipe> std::hash::Hash for MemRepository<Recipe>
where
    Recipe: spk_spec::Recipe + Send + Sync,
    Recipe::Output: Send + Sync,
{
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

impl<Recipe> PartialEq for MemRepository<Recipe>
where
    Recipe: spk_spec::Recipe + Send + Sync,
    Recipe::Output: Send + Sync,
{
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl<Recipe> Eq for MemRepository<Recipe>
where
    Recipe: spk_spec::Recipe + Send + Sync,
    Recipe::Output: Send + Sync,
{
}

#[async_trait::async_trait]
impl<Recipe> Repository for MemRepository<Recipe>
where
    Recipe: spk_spec::Recipe<Ident = Ident> + Clone + Send + Sync,
    Recipe::Output: spk_spec::Package<Ident = Ident> + Clone + Send + Sync,
{
    type Recipe = Recipe;

    fn address(&self) -> &url::Url {
        &self.address
    }

    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>> {
        Ok(self
            .specs
            .read()
            .await
            .keys()
            .map(|s| s.to_owned())
            .collect())
    }

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>> {
        if let Some(specs) = self.specs.read().await.get(name) {
            Ok(Arc::new(
                specs.keys().map(|v| Arc::new(v.to_owned())).collect(),
            ))
        } else {
            Ok(Arc::new(Vec::new()))
        }
    }

    async fn list_package_builds(&self, pkg: &Ident) -> Result<Vec<Ident>> {
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

    async fn list_build_components(&self, pkg: &Ident) -> Result<Vec<Component>> {
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

    fn name(&self) -> &RepositoryName {
        self.name.as_ref()
    }
    async fn read_recipe(&self, pkg: &Ident) -> Result<Arc<Self::Recipe>> {
        if pkg.build.is_some() {
            return Err(format!("cannot read a recipe for a package build: {pkg}").into());
        }
        self.specs
            .read()
            .await
            .get(&pkg.name)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(pkg.clone()))
            })?
            .get(&pkg.version)
            .map(Arc::clone)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(pkg.clone()))
            })
    }

    async fn read_components(&self, pkg: &Ident) -> Result<ComponentMap> {
        match &pkg.build {
            None => Err(Error::SpkValidatorsError(
                spk_validators::Error::PackageNotFoundError(pkg.clone()),
            )),
            Some(build) => self
                .packages
                .read()
                .await
                .get(&pkg.name)
                .ok_or_else(|| {
                    Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(
                        pkg.clone(),
                    ))
                })?
                .get(&pkg.version)
                .ok_or_else(|| {
                    Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(
                        pkg.clone(),
                    ))
                })?
                .get(build)
                .map(|(_, d)| d.to_owned())
                .ok_or_else(|| {
                    Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(
                        pkg.clone(),
                    ))
                }),
        }
    }

    async fn force_publish_recipe(&self, spec: &Self::Recipe) -> Result<()> {
        let mut specs = self.specs.write().await;
        let versions = specs.entry(spec.name().to_owned()).or_default();
        versions.remove(spec.version());
        drop(specs); // this lock will be needed to publish
        self.publish_recipe(spec).await
    }

    async fn publish_recipe(&self, spec: &Self::Recipe) -> Result<()> {
        let mut specs = self.specs.write().await;
        let versions = specs.entry(spec.name().to_owned()).or_default();
        if versions.contains_key(spec.version()) {
            Err(Error::SpkValidatorsError(
                spk_validators::Error::VersionExistsError(spec.to_ident()),
            ))
        } else {
            versions.insert(spec.version().clone(), Arc::new(spec.clone()));
            Ok(())
        }
    }

    async fn remove_recipe(&self, pkg: &Ident) -> Result<()> {
        let mut specs = self.specs.write().await;
        let versions = match specs.get_mut(&pkg.name) {
            Some(v) => v,
            None => {
                return Err(Error::SpkValidatorsError(
                    spk_validators::Error::PackageNotFoundError(pkg.clone()),
                ))
            }
        };
        if versions.remove(&pkg.version).is_none() {
            Err(Error::SpkValidatorsError(
                spk_validators::Error::PackageNotFoundError(pkg.clone()),
            ))
        } else {
            Ok(())
        }
    }

    async fn read_package(
        &self,
        // TODO: use an ident type that must have a build
        pkg: &Ident,
    ) -> Result<Arc<<Self::Recipe as spk_spec::Recipe>::Output>> {
        let build = pkg.build.as_ref().ok_or_else(|| {
            Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(pkg.clone()))
        })?;
        self.packages
            .read()
            .await
            .get(&pkg.name)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(pkg.clone()))
            })?
            .get(&pkg.version)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(pkg.clone()))
            })?
            .get(build)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(pkg.clone()))
            })
            .map(|found| Arc::clone(&found.0))
    }

    async fn publish_package(
        &self,
        spec: &<Self::Recipe as spk_spec::Recipe>::Output,
        components: &ComponentMap,
    ) -> Result<()> {
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

        builds.insert(build, (Arc::new(spec.clone()), components.clone()));
        Ok(())
    }

    async fn remove_package(&self, pkg: &Ident) -> Result<()> {
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
            None => {
                return Err(Error::SpkValidatorsError(
                    spk_validators::Error::PackageNotFoundError(pkg.clone()),
                ))
            }
        };

        let builds = match versions.get_mut(&pkg.version) {
            Some(v) => v,
            None => {
                return Err(Error::SpkValidatorsError(
                    spk_validators::Error::PackageNotFoundError(pkg.clone()),
                ))
            }
        };

        if builds.remove(build).is_none() {
            Err(Error::SpkValidatorsError(
                spk_validators::Error::PackageNotFoundError(pkg.clone()),
            ))
        } else {
            Ok(())
        }
    }
}
