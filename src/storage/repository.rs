// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{collections::HashMap, sync::Arc};

use crate::{api, prelude::*, Result};

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

#[async_trait::async_trait]
pub trait Repository: Sync {
    type Recipe: api::Recipe;

    /// A repository's address should identify it uniquely. It's
    /// expected that two handles to the same logical repository
    /// share an address
    fn address(&self) -> &url::Url;

    /// Return the set of known packages in this repo.
    async fn list_packages(&self) -> Result<Vec<api::PkgNameBuf>>;

    /// Return the set of versions available for the named package.
    async fn list_package_versions(
        &self,
        name: &api::PkgName,
    ) -> Result<Arc<Vec<Arc<api::Version>>>>;

    /// Return the set of builds for the given package name and version.
    async fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>>;

    /// Returns the set of components published for a package build
    async fn list_build_components(&self, pkg: &api::Ident) -> Result<Vec<api::Component>>;

    /// Return the repository's name, as in "local" or its name in the config file.
    fn name(&self) -> &api::RepositoryName;

    /// Read a package recipe for the given package, and version.
    ///
    /// # Errors:
    /// - PackageNotFoundError: If the package, or version does not exist
    async fn read_recipe(&self, pkg: &api::Ident) -> Result<Arc<Self::Recipe>>;

    /// Publish a package spec to this repository.
    ///
    /// The published recipe represents all builds of a single version.
    /// The source package, or at least one binary package should be
    /// published as well in order to make the package useful.
    ///
    /// # Errors:
    /// - VersionExistsError: if the recipe version is already present
    async fn publish_recipe(&self, spec: &Self::Recipe) -> Result<()>;

    /// Remove a package recipe from this repository.
    ///
    /// This will not remove builds for this package, but will make it unresolvable
    /// and unsearchable. It's recommended that you remove all existing builds
    /// before removing the recipe in order to keep the repository clean.
    async fn remove_recipe(&self, pkg: &api::Ident) -> Result<()>;

    /// Publish a package recipe to this repository.
    ///
    /// Same as [`Self::publish_recipe`] except that it clobbers any existing
    /// recipe with the same version.
    async fn force_publish_recipe(&self, spec: &Self::Recipe) -> Result<()>;

    /// Read package information for a specific version and build.
    ///
    /// # Errors:
    /// - PackageNotFoundError: If the package, version, or build does not exist
    async fn read_package(
        &self,
        // TODO: use an ident type that must have a build
        pkg: &api::Ident,
    ) -> Result<Arc<<Self::Recipe as api::Recipe>::Output>>;

    /// Publish a package to this repository.
    ///
    /// The provided component digests are expected to each identify an spfs
    /// layer which contains properly constructed binary package files and metadata.
    async fn publish_package(
        &self,
        package: &<Self::Recipe as api::Recipe>::Output,
        components: &HashMap<api::Component, spfs::encoding::Digest>,
    ) -> Result<()>;

    /// Modify a package in this repository.
    ///
    /// The provided package must already exist. This method is unsafe
    /// and generally should only be used to modify components that
    /// do not change the structure of the package (such as metadata
    /// or deprecation status).
    async fn update_package(&self, package: &<Self::Recipe as api::Recipe>::Output) -> Result<()> {
        let components = self.read_components(package.ident()).await?;
        self.publish_package(package, &components).await
    }

    /// Remove a package from this repository.
    ///
    /// The given package identifier must identify a full package build.
    async fn remove_package(
        &self,
        // TODO: use an ident type that must have a build
        pkg: &api::Ident,
    ) -> Result<()>;

    /// Identify the payloads for the identified package's components.
    async fn read_components(
        &self,
        // TODO: use an ident type that must have a build
        pkg: &api::Ident,
    ) -> Result<HashMap<api::Component, spfs::encoding::Digest>>;

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
