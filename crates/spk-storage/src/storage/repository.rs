// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{collections::HashMap, sync::Arc};

use crate::{Error, Result};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName};
use spk_schema::foundation::spec_ops::PackageOps;
use spk_schema::foundation::version::Version;
use spk_schema::ident_build::{Build, EmbeddedSource, InvalidBuildError};
use spk_schema::Ident;
use spk_schema::{Package, Recipe};

#[cfg(test)]
#[path = "./repository_test.rs"]
mod repository_test;

#[derive(Clone, Copy, Debug)]
pub enum CachePolicy {
    CacheOk,
    BypassCache,
}

impl CachePolicy {
    /// Return true if the policy allows for a cached result.
    pub fn cached_result_permitted(&self) -> bool {
        matches!(self, CachePolicy::CacheOk)
    }
}

/// Policy for publishing recipes.
#[derive(Clone, Copy, Debug)]
pub enum PublishPolicy {
    OverwriteVersion,
    DoNotOverwriteVersion,
}

/// Low level storage operations.
///
/// These methods are expected to have different implementations for different
/// storage types, but perform the same logical operation for any storage type.
#[async_trait::async_trait]
pub trait Storage: Sync {
    type Recipe: spk_schema::Recipe<Output = Self::Package, Recipe = Self::Recipe>;
    type Package: Package<Input = Self::Recipe, Ident = Ident>;

    /// Publish a package to this repository.
    ///
    /// The provided component digests are expected to each identify an spfs
    /// layer which contains properly constructed binary package files and metadata.
    async fn publish_package_to_storage(
        &self,
        package: &<Self::Recipe as spk_schema::Recipe>::Output,
        components: &HashMap<Component, spfs::encoding::Digest>,
    ) -> Result<()>;

    /// Publish a package spec to this repository.
    ///
    /// The published spec represents all builds of a single version.
    /// The source package, or at least one binary package should be
    /// published as well in order to make the spec usable in environments.
    ///
    /// # Errors:
    /// - VersionExistsError: if the spec version is already present and
    ///   `publish_policy` does not allow overwrite.
    async fn publish_recipe_to_storage(
        &self,
        spec: &Self::Recipe,
        publish_policy: PublishPolicy,
    ) -> Result<()>;

    /// Identify the payloads for the identified package's components.
    async fn read_components_from_storage(
        &self,
        pkg: &Ident,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>>;

    /// Remove a package from this repository.
    ///
    /// The given package identifier must identify a full package build.
    async fn remove_package_from_storage(&self, pkg: &Ident) -> Result<()>;
}

/// High level repository concepts.
///
/// An abstraction for interacting with different storage backends as a
/// repository for spk packages.
#[async_trait::async_trait]
pub trait Repository: Storage + Sync {
    /// A repository's address should identify it uniquely. It's
    /// expected that two handles to the same logical repository
    /// share an address
    fn address(&self) -> &url::Url;

    /// Return the set of known packages in this repo.
    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>>;

    /// Return the set of versions available for the named package.
    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>>;

    /// Return the set of builds for the given package name and version.
    async fn list_package_builds(&self, pkg: &Ident) -> Result<Vec<Ident>>;

    /// Returns the set of components published for a package build
    async fn list_build_components(&self, pkg: &Ident) -> Result<Vec<Component>>;

    /// Return the repository's name, as in "local" or its name in the config file.
    fn name(&self) -> &RepositoryName;

    /// Read a package recipe for the given package, and version.
    ///
    /// # Errors:
    /// - PackageNotFoundError: If the package, or version does not exist
    async fn read_recipe(&self, pkg: &Ident) -> Result<Arc<Self::Recipe>>;

    /// Publish a package spec to this repository.
    ///
    /// The published recipe represents all builds of a single version.
    /// The source package, or at least one binary package should be
    /// published as well in order to make the package useful.
    ///
    /// # Errors:
    /// - VersionExistsError: if the recipe version is already present
    async fn publish_recipe(&self, spec: &Self::Recipe) -> Result<()> {
        self.publish_recipe_to_storage(spec, PublishPolicy::DoNotOverwriteVersion)
            .await
    }

    /// Remove a package recipe from this repository.
    ///
    /// This will not remove builds for this package, but will make it unresolvable
    /// and unsearchable. It's recommended that you remove all existing builds
    /// before removing the recipe in order to keep the repository clean.
    async fn remove_recipe(&self, pkg: &Ident) -> Result<()>;

    /// Publish a package spec to this repository.
    ///
    /// Same as [`Self::publish_recipe`] except that it clobbers any existing
    /// recipe with the same version.
    async fn force_publish_recipe(&self, spec: &Self::Recipe) -> Result<()> {
        self.publish_recipe_to_storage(spec, PublishPolicy::OverwriteVersion)
            .await
    }

    /// Read package information for a specific version and build.
    ///
    /// # Errors:
    /// - PackageNotFoundError: If the package, version, or build does not exist
    async fn read_package(
        &self,
        // TODO: use an ident type that must have a build
        pkg: &Ident,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>>;

    /// Publish a package to this repository.
    ///
    /// The provided component digests are expected to each identify an spfs
    /// layer which contains properly constructed binary package files and metadata.
    async fn publish_package(
        &self,
        package: &<<Self as Storage>::Recipe as spk_schema::Recipe>::Output,
        components: &HashMap<Component, spfs::encoding::Digest>,
    ) -> Result<()>
    where
        <<<Self as Storage>::Recipe as spk_schema::Recipe>::Output as Package>::Input: Clone,
    {
        let build = match &package.ident().build {
            Some(b) => b.to_owned(),
            None => {
                return Err(Error::String(format!(
                    "Package must include a build in order to be published: {}",
                    package.ident()
                )))
            }
        };

        if let Build::Embedded(_) = build {
            return Err(Error::SpkIdentBuildError(InvalidBuildError::new_error(
                "Cannot publish embedded package".to_string(),
            )));
        }

        self.publish_package_to_storage(package, components).await?;

        // After successfully publishing a package, also publish stubs for any
        // embedded packages in this package.
        if !package.ident().is_source() {
            for embed in package.embedded_as_recipes()?.into_iter() {
                // The "version spec" must exist for this package to be discoverable.
                // One may already exist from "real" (non-embedded) publishes.
                let version_spec = embed.with_build(None);
                match self.publish_recipe(&version_spec).await {
                    Ok(_)
                    | Err(Error::SpkValidatorsError(
                        spk_schema::validators::Error::VersionExistsError(_),
                    )) => {}
                    Err(err) => return Err(err),
                };

                let embed = embed.with_build(Some(Build::Embedded(EmbeddedSource::Ident(
                    package.ident().to_string(),
                ))));
                self.force_publish_recipe(&embed).await?;
            }
        }

        Ok(())
    }

    /// Modify a package in this repository.
    ///
    /// The provided package must already exist. This method is unsafe
    /// and generally should only be used to modify components that
    /// do not change the structure of the package (such as metadata
    /// or deprecation status).
    async fn update_package(
        &self,
        package: &<Self::Recipe as spk_schema::Recipe>::Output,
    ) -> Result<()>
    where
        <Self::Recipe as spk_schema::Recipe>::Output: Package<Ident = Ident>,
    {
        let components = self.read_components(package.ident()).await?;
        self.publish_package(package, &components).await
    }

    /// Remove a package from this repository.
    ///
    /// The given package identifier must identify a full package build.
    async fn remove_package(
        &self,
        // TODO: use an ident type that must have a build
        pkg: &Ident,
    ) -> Result<()> {
        if pkg.build.is_none() {
            return Err(Error::String(format!(
                "Package must include a build in order to be removed: {}",
                pkg
            )));
        }

        self.remove_package_from_storage(pkg).await
    }

    /// Identify the payloads for the identified package's components.
    async fn read_components(
        &self,
        // TODO: use an ident type that must have a build
        pkg: &Ident,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        self.read_components_from_storage(pkg).await
    }

    /// Perform any upgrades that are pending on this repository.
    ///
    /// This will bring the repository up-to-date for the current
    /// spk library version, but may also make it incompatible with
    /// older ones. Upgrades can also take time depending on their
    /// nature and the size of the repository. Please, take time to
    /// read any release and upgrade notes before invoking this.
    async fn upgrade(&self) -> Result<String> {
        Ok("Nothing to do.".to_string())
    }

    /// Change the active cache policy.
    ///
    /// The old cache policy is returned. Not all storage types may support
    /// caching, and calling this may be ignored.
    fn set_cache_policy(&self, _cache_policy: CachePolicy) -> CachePolicy {
        CachePolicy::BypassCache
    }
}

/// Change the active cache policy while running a block of code.
#[macro_export]
macro_rules! with_cache_policy {
    ($repo:expr, $cp:expr, $expr:block ) => {{
        let repo = &$repo;
        let old_cache_policy = repo.set_cache_policy($cp);
        let r = $expr;
        repo.set_cache_policy(old_cache_policy);
        r
    }};
}
