// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::sync::Arc;

use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName, RepositoryNameBuf};
use spk_schema::foundation::version::Version;
use spk_schema::{BuildIdent, Spec, SpecRecipe, VersionIdent};
use tokio::sync::RwLock;

use super::Repository;
use super::repository::{PublishPolicy, Storage};
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
            name: format!("mem-{}", ulid::Ulid::new().to_string().to_lowercase())
                .try_into()
                .expect("valid repository name"),
            specs,
            packages: Arc::default(),
            embedded_stubs: Arc::default(),
            _marker: std::marker::PhantomData,
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
    Recipe: spk_schema::Recipe<Output = Package> + Send + Sync,
    Package: spk_schema::Package<Package = Package> + Send + Sync,
{
    type Recipe = Recipe;
    type Package = Package;

    async fn get_concrete_package_builds(&self, pkg: &VersionIdent) -> Result<HashSet<BuildIdent>> {
        if let Some(versions) = self.packages.read().await.get(pkg.name()) {
            if let Some(builds) = versions.get(pkg.version()) {
                Ok(builds
                    .keys()
                    .map(|b| pkg.to_build_ident(b.clone()))
                    .collect())
            } else {
                Ok(HashSet::new())
            }
        } else {
            Ok(HashSet::new())
        }
    }

    async fn get_embedded_package_builds(&self, pkg: &VersionIdent) -> Result<HashSet<BuildIdent>> {
        if let Some(versions) = self.embedded_stubs.read().await.get(pkg.name()) {
            if let Some(builds) = versions.get(pkg.version()) {
                Ok(builds
                    .keys()
                    .map(|b| pkg.to_build_ident(b.clone()))
                    .collect())
            } else {
                Ok(HashSet::new())
            }
        } else {
            Ok(HashSet::new())
        }
    }

    async fn publish_embed_stub_to_storage(&self, spec: &Self::Package) -> Result<()> {
        let build = spec.ident().build().to_owned();
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
        let build = package.ident().build().clone();

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
            Err(Error::VersionExists(spec.ident().clone()))
        } else {
            versions.insert(spec.version().clone(), Arc::new(spec.clone()));
            Ok(())
        }
    }

    async fn read_components_from_storage(&self, pkg: &BuildIdent) -> Result<ComponentMap> {
        self.packages
            .read()
            .await
            .get(pkg.name())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident())))?
            .get(pkg.version())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident())))?
            .get(pkg.build())
            .map(|(_, d)| d.to_owned())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident())))
    }

    async fn read_package_from_storage(
        &self,
        pkg: &BuildIdent,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>> {
        // https://github.com/rust-lang/rust-clippy/issues/12560
        #[allow(clippy::map_clone)]
        self.packages
            .read()
            .await
            .get(pkg.name())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident())))?
            .get(pkg.version())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident())))?
            .get(pkg.build())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident())))
            .map(|found| Arc::clone(&found.0))
    }

    async fn remove_embed_stub_from_storage(&self, pkg: &BuildIdent) -> Result<()> {
        if !pkg.build().is_embedded() {
            return Err(Error::String(format!(
                "Package must identify an embedded package in order to be removed: {pkg}",
            )));
        };

        let mut packages = self.embedded_stubs.write().await;
        match packages.get_mut(pkg.name()) {
            Some(versions) => {
                match versions.get_mut(pkg.version()) {
                    Some(builds) => {
                        if builds.remove(pkg.build()).is_none() {
                            return Err(Error::PackageNotFound(Box::new(pkg.to_any_ident())));
                        }
                        if builds.is_empty() {
                            versions.remove(pkg.version());
                        }
                    }
                    None => {
                        return Err(Error::PackageNotFound(Box::new(pkg.to_any_ident())));
                    }
                };
                if versions.is_empty() {
                    packages.remove(pkg.name());
                }
            }
            None => {
                return Err(Error::PackageNotFound(Box::new(pkg.to_any_ident())));
            }
        };
        Ok(())
    }

    async fn remove_package_from_storage(&self, pkg: &BuildIdent) -> Result<()> {
        let mut packages = self.packages.write().await;
        let versions = match packages.get_mut(pkg.name()) {
            Some(v) => v,
            None => return Err(Error::PackageNotFound(Box::new(pkg.to_any_ident()))),
        };

        let builds = match versions.get_mut(pkg.version()) {
            Some(v) => v,
            None => return Err(Error::PackageNotFound(Box::new(pkg.to_any_ident()))),
        };

        if builds.remove(pkg.build()).is_none() {
            Err(Error::PackageNotFound(Box::new(pkg.to_any_ident())))
        } else {
            Ok(())
        }
    }
}

#[async_trait::async_trait]
impl<Recipe, Package> Repository for MemRepository<Recipe, Package>
where
    Recipe: spk_schema::Recipe<Output = Package> + Clone + Send + Sync,
    Recipe::Output: spk_schema::Package + Clone + Send + Sync,
    Package: spk_schema::Package<Package = Package> + Send + Sync,
{
    fn address(&self) -> &url::Url {
        &self.address
    }

    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>> {
        let (specs, packages, embedded) = tokio::join!(
            self.specs.read(),
            self.packages.read(),
            self.embedded_stubs.read()
        );
        let mut names: HashSet<_> = specs.keys().map(|s| s.to_owned()).collect();
        names.extend(packages.keys().map(|s| s.to_owned()));
        names.extend(embedded.keys().map(|s| s.to_owned()));
        Ok(names.into_iter().collect())
    }

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>> {
        let (specs, packages, embedded) = tokio::join!(
            self.specs.read(),
            self.packages.read(),
            self.embedded_stubs.read(),
        );
        let mut versions = HashSet::new();
        if let Some(v) = specs.get(name) {
            versions.extend(v.keys().map(|s| s.to_owned()));
        }
        if let Some(v) = packages.get(name) {
            versions.extend(v.keys().map(|s| s.to_owned()));
        }
        if let Some(v) = embedded.get(name) {
            versions.extend(v.keys().map(|s| s.to_owned()));
        }

        Ok(Arc::new(versions.into_iter().map(Arc::new).collect()))
    }

    async fn list_build_components(&self, pkg: &BuildIdent) -> Result<Vec<Component>> {
        Ok(self
            .packages
            .read()
            .await
            .get(pkg.name())
            .and_then(|versions| versions.get(pkg.version()))
            .and_then(|builds| builds.get(pkg.build()))
            .map(|(_, build_map)| build_map)
            .map(|cmpts| cmpts.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default())
    }

    fn name(&self) -> &RepositoryName {
        self.name.as_ref()
    }

    async fn read_embed_stub(&self, pkg: &BuildIdent) -> Result<Arc<Self::Package>> {
        self.embedded_stubs
            .read()
            .await
            .get(pkg.name())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident())))?
            .get(pkg.version())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident())))?
            .get(pkg.build())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident())))
            .cloned()
    }

    async fn read_recipe(&self, pkg: &VersionIdent) -> Result<Arc<Self::Recipe>> {
        self.specs
            .read()
            .await
            .get(pkg.name())
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident(None))))?
            .get(pkg.version())
            .cloned()
            .ok_or_else(|| Error::PackageNotFound(Box::new(pkg.to_any_ident(None))))
    }

    async fn remove_recipe(&self, pkg: &VersionIdent) -> Result<()> {
        let mut specs = self.specs.write().await;
        match specs.get_mut(pkg.name()) {
            Some(versions) => {
                if versions.remove(pkg.version()).is_none() {
                    return Err(Error::PackageNotFound(Box::new(pkg.to_any_ident(None))));
                }
                if versions.is_empty() {
                    specs.remove(pkg.name());
                }
            }
            None => return Err(Error::PackageNotFound(Box::new(pkg.to_any_ident(None)))),
        };
        Ok(())
    }
}
