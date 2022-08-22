// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::sync::Arc;

use spk_schema::foundation::ident_build::{Build, EmbeddedSource};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName, RepositoryNameBuf};
use spk_schema::foundation::version::Version;
use spk_schema::Ident;
use spk_schema::{Spec, SpecRecipe};
use tokio::sync::RwLock;

use super::repository::{PublishPolicy, Storage};
use super::Repository;
use crate::{Error, Result};

type ComponentMap = HashMap<Component, spfs::encoding::Digest>;
type PackageMap<T> = HashMap<PkgNameBuf, VersionMap<T>>;
type VersionMap<T> = HashMap<Version, T>;
type BuildMap<Package> = HashMap<Build, (Arc<Package>, ComponentMap)>;
type StubMap<Package> = HashMap<Build, Arc<Package>>;

#[derive(Clone, Debug)]
pub struct MemRepository<Recipe = SpecRecipe, Package = Spec>
where
    Recipe: spk_schema::Recipe + Sync + Send,
    Recipe::Output: Sync + Send,
{
    address: url::Url,
    name: RepositoryNameBuf,
    specs: Arc<RwLock<PackageMap<Arc<Recipe>>>>,
    packages: Arc<RwLock<PackageMap<BuildMap<Recipe::Output>>>>,
    embedded_stubs: Arc<RwLock<PackageMap<StubMap<Package>>>>,
    _marker: std::marker::PhantomData<Package>,
}

impl<Recipe, Package> MemRepository<Recipe>
where
    Recipe: spk_schema::Recipe<Output = Package> + Send + Sync,
    Package: spk_schema::Package + Send + Sync,
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
            embedded_stubs: Arc::default(),
            _marker: std::marker::PhantomData::default(),
        }
    }
}

impl<Recipe, Package> Default for MemRepository<Recipe>
where
    Recipe: spk_schema::Recipe<Output = Package> + Send + Sync,
    Package: spk_schema::Package + Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Recipe> std::hash::Hash for MemRepository<Recipe>
where
    Recipe: spk_schema::Recipe + Send + Sync,
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
    Recipe: spk_schema::Recipe + Send + Sync,
    Recipe::Output: Send + Sync,
{
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl<Recipe> Eq for MemRepository<Recipe> where Recipe: spk_schema::Recipe + Send + Sync {}

#[async_trait::async_trait]
impl<Recipe, Package> Storage for MemRepository<Recipe, Package>
where
    Recipe: spk_schema::Recipe<Output = Package, Ident = Ident> + Send + Sync,
    Package: spk_schema::Package<Input = Recipe, Ident = Ident, Package = Package> + Send + Sync,
{
    type Recipe = Recipe;
    type Package = Package;

    async fn get_concrete_package_builds(&self, pkg: &Ident) -> Result<HashSet<Ident>> {
        if let Some(versions) = self.packages.read().await.get(&pkg.name) {
            if let Some(builds) = versions.get(&pkg.version) {
                Ok(builds
                    .keys()
                    .map(|b| pkg.with_build(Some(b.clone())))
                    .collect())
            } else {
                Ok(HashSet::new())
            }
        } else {
            Ok(HashSet::new())
        }
    }

    async fn get_embedded_package_builds(&self, pkg: &Ident) -> Result<HashSet<Ident>> {
        if let Some(versions) = self.embedded_stubs.read().await.get(&pkg.name) {
            if let Some(builds) = versions.get(&pkg.version) {
                Ok(builds
                    .keys()
                    .map(|b| pkg.with_build(Some(b.clone())))
                    .collect())
            } else {
                Ok(HashSet::new())
            }
        } else {
            Ok(HashSet::new())
        }
    }

    async fn publish_embed_stub_to_storage(&self, spec: &Self::Package) -> Result<()> {
        let build = match &spec.ident().build {
            Some(b) => b.to_owned(),
            None => {
                return Err(Error::String(format!(
                    "Package must include a build in order to be published: {}",
                    spec.ident()
                )))
            }
        };

        let mut embedded_stubs = self.embedded_stubs.write().await;
        let versions = embedded_stubs.entry(spec.name().to_owned()).or_default();
        let builds = versions.entry(spec.version().clone()).or_default();

        builds.insert(build, Arc::new(spec.clone()));
        Ok(())
    }

    async fn publish_package_to_storage(
        &self,
        package: &<Self::Recipe as spk_schema::Recipe>::Output,
        components: &ComponentMap,
    ) -> Result<()> {
        // Caller has already proven that build is `Some`.
        let build = package.ident().build.as_ref().unwrap().clone();

        let mut packages = self.packages.write().await;
        let versions = packages.entry(package.name().to_owned()).or_default();
        let builds = versions.entry(package.version().clone()).or_default();

        builds.insert(build, (Arc::new(package.clone()), components.clone()));
        Ok(())
    }

    async fn publish_recipe_to_storage(
        &self,
        spec: &Self::Recipe,
        publish_policy: PublishPolicy,
    ) -> Result<()> {
        let mut specs = self.specs.write().await;
        let versions = specs.entry(spec.name().to_owned()).or_default();
        if matches!(publish_policy, PublishPolicy::DoNotOverwriteVersion)
            && versions.contains_key(spec.version())
        {
            Err(Error::SpkValidatorsError(
                spk_schema::validators::Error::VersionExistsError(spec.to_ident()),
            ))
        } else {
            versions.insert(spec.version().clone(), Arc::new(spec.clone()));
            Ok(())
        }
    }

    async fn read_components_from_storage(&self, pkg: &Ident) -> Result<ComponentMap> {
        match &pkg.build {
            None => Err(Error::SpkValidatorsError(
                spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
            )),
            Some(build) => self
                .packages
                .read()
                .await
                .get(&pkg.name)
                .ok_or_else(|| {
                    Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                        pkg.clone(),
                    ))
                })?
                .get(&pkg.version)
                .ok_or_else(|| {
                    Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                        pkg.clone(),
                    ))
                })?
                .get(build)
                .map(|(_, d)| d.to_owned())
                .ok_or_else(|| {
                    Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                        pkg.clone(),
                    ))
                }),
        }
    }

    async fn read_package_from_storage(
        &self,
        // TODO: use an ident type that must have a build
        pkg: &Ident,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>> {
        let build = pkg.build.as_ref().ok_or_else(|| {
            Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                pkg.clone(),
            ))
        })?;
        self.packages
            .read()
            .await
            .get(&pkg.name)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.clone(),
                ))
            })?
            .get(&pkg.version)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.clone(),
                ))
            })?
            .get(build)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.clone(),
                ))
            })
            .map(|found| Arc::clone(&found.0))
    }

    async fn remove_embed_stub_from_storage(&self, pkg: &Ident) -> Result<()> {
        let build = match &pkg.build {
            Some(b @ Build::Embedded(EmbeddedSource::Package { .. })) => b,
            _ => {
                return Err(Error::String(format!(
                    "Package must identify an embedded package in order to be removed: {}",
                    pkg
                )))
            }
        };

        let mut packages = self.embedded_stubs.write().await;
        match packages.get_mut(&pkg.name) {
            Some(versions) => {
                match versions.get_mut(&pkg.version) {
                    Some(builds) => {
                        if builds.remove(build).is_none() {
                            return Err(Error::SpkValidatorsError(
                                spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
                            ));
                        }
                        if builds.is_empty() {
                            versions.remove(&pkg.version);
                        }
                    }
                    None => {
                        return Err(Error::SpkValidatorsError(
                            spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
                        ))
                    }
                };
                if versions.is_empty() {
                    packages.remove(&pkg.name);
                }
            }
            None => {
                return Err(Error::SpkValidatorsError(
                    spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
                ))
            }
        };
        Ok(())
    }

    async fn remove_package_from_storage(&self, pkg: &Ident) -> Result<()> {
        // Caller has already proven that build is `Some`.
        let build = pkg.build.as_ref().unwrap();

        let mut packages = self.packages.write().await;
        let versions = match packages.get_mut(&pkg.name) {
            Some(v) => v,
            None => {
                return Err(Error::SpkValidatorsError(
                    spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
                ))
            }
        };

        let builds = match versions.get_mut(&pkg.version) {
            Some(v) => v,
            None => {
                return Err(Error::SpkValidatorsError(
                    spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
                ))
            }
        };

        if builds.remove(build).is_none() {
            Err(Error::SpkValidatorsError(
                spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
            ))
        } else {
            Ok(())
        }
    }
}

#[async_trait::async_trait]
impl<Recipe, Package> Repository for MemRepository<Recipe, Package>
where
    Recipe: spk_schema::Recipe<Ident = Ident, Output = Package> + Clone + Send + Sync,
    Recipe::Output: spk_schema::Package<Ident = Ident> + Clone + Send + Sync,
    Package: spk_schema::Package<Input = Recipe, Package = Package> + Send + Sync,
{
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

    async fn read_embed_stub(&self, pkg: &Ident) -> Result<Arc<Self::Package>> {
        let build = pkg.build.as_ref().ok_or_else(|| {
            Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                pkg.clone(),
            ))
        })?;
        self.embedded_stubs
            .read()
            .await
            .get(&pkg.name)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.clone(),
                ))
            })?
            .get(&pkg.version)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.clone(),
                ))
            })?
            .get(build)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.clone(),
                ))
            })
            .map(Arc::clone)
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
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.clone(),
                ))
            })?
            .get(&pkg.version)
            .map(Arc::clone)
            .ok_or_else(|| {
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.clone(),
                ))
            })
    }

    async fn remove_recipe(&self, pkg: &Ident) -> Result<()> {
        let mut specs = self.specs.write().await;
        match specs.get_mut(&pkg.name) {
            Some(versions) => {
                if versions.remove(&pkg.version).is_none() {
                    return Err(Error::SpkValidatorsError(
                        spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
                    ));
                }
                if versions.is_empty() {
                    specs.remove(&pkg.name);
                }
            }
            None => {
                return Err(Error::SpkValidatorsError(
                    spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
                ))
            }
        };
        Ok(())
    }
}
