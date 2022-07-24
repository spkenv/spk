// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{Error, Result};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName};
use spk_schema::foundation::spec_ops::PackageOps;
use spk_schema::foundation::version::Version;
use spk_schema::ident_build::EmbeddedSource;
use spk_schema::ident_build::{Build, InvalidBuildError};
use spk_schema::Ident;
use spk_schema::{Deprecate, DeprecateMut, Package};

use self::internal::RepositoryExt;

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
    type Recipe: spk_schema::Recipe<Output = Self::Package, Recipe = Self::Recipe, Ident = Ident>;
    type Package: Package<Input = Self::Recipe, Ident = Ident>;

    /// Return the set of concrete builds for the given package name and version.
    ///
    /// This method should not return any embedded package stub builds.
    async fn get_concrete_package_builds(&self, pkg: &Ident) -> Result<HashSet<Ident>>;

    /// Return the set of embedded stub builds for the given package name and version.
    ///
    /// This method should only return embedded package stub builds.
    async fn get_embedded_package_builds(&self, pkg: &Ident) -> Result<HashSet<Ident>>;

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

pub(in crate::storage) mod internal {
    use std::collections::{BTreeSet, HashMap};

    use spk_schema::foundation::{
        ident_build::{Build, EmbeddedSource, EmbeddedSourcePackage},
        ident_component::Component,
        spec_ops::{PackageOps, RecipeOps},
    };
    use spk_schema::{Deprecate, DeprecateMut, Package, Recipe};

    use crate::{with_cache_policy, CachePolicy, Error, Result};

    /// Reusable methods for [`super::Repository`] that are not intended to be
    /// part of its public interface.
    #[async_trait::async_trait]
    pub trait RepositoryExt: super::Repository {
        /// Add a [`Recipe`] to a repository to represent an embedded package.
        ///
        /// This spec is a link back to the package that embeds it, and allows
        /// the repository to advertise its existence.
        async fn create_embedded_stub_for_spec(
            &self,
            spec_for_parent: &Self::Package,
            spec_for_embedded_pkg: &Self::Recipe,
            components_that_embed_this_pkg: BTreeSet<Component>,
        ) -> Result<()>
        where
            Self::Recipe: DeprecateMut,
        {
            // The "version spec" must exist for this package to be discoverable.
            // One may already exist from "real" (non-embedded) publishes.
            let version_spec = spec_for_embedded_pkg.with_build(None);
            match self.publish_recipe(&version_spec).await {
                Ok(_)
                | Err(Error::SpkValidatorsError(
                    spk_schema::validators::Error::VersionExistsError(_),
                )) => {}
                Err(err) => return Err(err),
            };

            let mut spec_for_embedded_pkg = spec_for_embedded_pkg.with_build(Some(
                Build::Embedded(EmbeddedSource::Package(Box::new(EmbeddedSourcePackage {
                    ident: spec_for_parent.ident().into(),
                    components: components_that_embed_this_pkg,
                }))),
            ));
            spec_for_embedded_pkg.set_deprecated(spec_for_parent.is_deprecated())?;
            self.force_publish_recipe(&spec_for_embedded_pkg).await
        }

        /// Get all the embedded packages described by a [`Package`] and
        /// return what [`Component`]s are providing each one.
        fn get_embedded_providers(
            &self,
            package: &<Self::Recipe as Recipe>::Output,
        ) -> Result<HashMap<<Self as super::Storage>::Recipe, BTreeSet<Component>>> {
            let mut embedded_providers = HashMap::new();
            for (embed, component) in package.embedded_as_recipes()?.into_iter() {
                // "top level" embedded as assumed to be provided by the "run"
                // component.
                (*embedded_providers
                    .entry(embed)
                    .or_insert_with(BTreeSet::new))
                .insert(component.unwrap_or(Component::Run));
            }
            Ok(embedded_providers)
        }

        /// Remove the [`Recipe`] from a repository that represents the
        /// embedded package of some other package.
        ///
        /// The stub should be removed when the package that had been embedding
        /// the package is removed or modified such that it no longer embeds
        /// this package.
        async fn remove_embedded_stub_for_spec(
            &self,
            spec_for_parent: &Self::Package,
            spec_for_embedded_pkg: &Self::Recipe,
            components_that_embed_this_pkg: BTreeSet<Component>,
        ) -> Result<()> {
            let spec_for_embedded_pkg =
                spec_for_embedded_pkg
                    .to_ident()
                    .with_build(Some(Build::Embedded(EmbeddedSource::Package(Box::new(
                        EmbeddedSourcePackage {
                            ident: spec_for_parent.ident().into(),
                            components: components_that_embed_this_pkg,
                        },
                    )))));
            self.remove_recipe(&spec_for_embedded_pkg.with_build(None))
                .await?;

            // If this was the last stub and there are no other builds, remove
            // the "version spec".
            if let Ok(builds) = with_cache_policy!(self, CachePolicy::BypassCache, {
                self.list_package_builds(&spec_for_embedded_pkg)
            })
            .await
            {
                if builds.is_empty() {
                    let version_spec = spec_for_embedded_pkg.with_build(None);
                    if let Err(err) = self.remove_recipe(&version_spec).await {
                        tracing::warn!(
                            ?spec_for_embedded_pkg,
                            ?err,
                            "Failed to remove version spec after removing last embed stub"
                        );
                    }
                }
            }

            Ok(())
        }
    }
}

/// Blanket implementation.
impl<T> internal::RepositoryExt for T where T: Repository + ?Sized {}

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
    async fn list_package_builds(&self, pkg: &Ident) -> Result<Vec<Ident>> {
        let concrete_builds = self.get_concrete_package_builds(pkg);
        let embedded_builds = self.get_embedded_package_builds(pkg);
        let (mut concrete, embedded) = tokio::try_join!(concrete_builds, embedded_builds)?;
        concrete.extend(embedded.into_iter());
        Ok(concrete.into_iter().collect())
    }

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
        Self::Recipe: DeprecateMut,
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
        if package.ident().can_embed() {
            let embedded_providers = self.get_embedded_providers(package)?;

            for (embed, components) in embedded_providers.into_iter() {
                self.create_embedded_stub_for_spec(package, &embed, components)
                    .await?;
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
        <Self::Recipe as spk_schema::Recipe>::Output: Package<Ident = Ident> + Deprecate,
        Self::Recipe: DeprecateMut,
    {
        // Read the contents of the existing spec, if any, before it is
        // overwritten.
        let original_spec = if package.ident().can_embed() {
            Some(self.read_package(package.ident()).await)
        } else {
            None
        };

        let components = self.read_components(package.ident()).await?;
        if let Err(err) = self.publish_package_to_storage(package, &components).await {
            return Err(err);
        }

        // Changes that affect embedded stubs:
        // - change in deprecation status
        // - adding/removing embedded packages
        if let Some(Ok(original_spec)) = original_spec {
            let original_embedded_providers = self.get_embedded_providers(&*original_spec)?;
            let new_embedded_providers = self.get_embedded_providers(package)?;
            // No change case #1: no embedded packages involved.
            if original_embedded_providers.is_empty() && new_embedded_providers.is_empty() {
                return Ok(());
            }
            // No change case #2: no embedded packages changes and no deprecation
            // status changes.
            let embedded_providers_have_changed =
                original_embedded_providers != new_embedded_providers;
            if !embedded_providers_have_changed
                && original_spec.is_deprecated() != package.is_deprecated()
            {
                // Update all stubs to change their deprecation status too.
                for (embed, components) in new_embedded_providers.into_iter() {
                    self.create_embedded_stub_for_spec(package, &embed, components)
                        .await?
                }
            } else if embedded_providers_have_changed {
                let original_keys: HashSet<&Self::Recipe> =
                    original_embedded_providers.keys().collect();
                let new_keys: HashSet<&Self::Recipe> = new_embedded_providers.keys().collect();

                // First deal with embeds that appeared or disappeared.
                for added_or_removed_spec in original_keys.symmetric_difference(&new_keys) {
                    if original_keys.contains(added_or_removed_spec) {
                        // This embed was removed
                        if let Some(components) =
                            original_embedded_providers.get(*added_or_removed_spec)
                        {
                            self.remove_embedded_stub_for_spec(
                                package,
                                *added_or_removed_spec,
                                components.clone(),
                            )
                            .await?
                        }
                    } else {
                        // This embed was added
                        if let Some(components) = new_embedded_providers.get(*added_or_removed_spec)
                        {
                            self.create_embedded_stub_for_spec(
                                package,
                                *added_or_removed_spec,
                                components.clone(),
                            )
                            .await?
                        }
                    }
                }

                // For any embeds that are unchanged, update the deprecation
                // status if it has changed.
                if original_spec.is_deprecated() == package.is_deprecated() {
                    return Ok(());
                }

                for returning_spec in original_keys.intersection(&new_keys) {
                    if let Some(components) = new_embedded_providers.get(*returning_spec) {
                        self.create_embedded_stub_for_spec(
                            package,
                            *returning_spec,
                            components.clone(),
                        )
                        .await?
                    }
                }
            }
        }
        // else if there was no original spec, assume there is nothing needed
        // to do.

        Ok(())
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

        // Attempt to find and remove any related embedded package stubs.
        if let Ok(spec) = self.read_package(pkg).await {
            if spec.ident().can_embed() {
                let embedded_providers = self.get_embedded_providers(&*spec)?;

                for (embed, components) in embedded_providers.into_iter() {
                    self.remove_embedded_stub_for_spec(&*spec, &embed, components)
                        .await?
                }
            }
        }

        self.remove_package_from_storage(pkg).await
    }

    /// Identify the payloads for this identified package's components.
    async fn read_components(
        &self,
        // TODO: use an ident type that must have a build
        pkg: &Ident,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        if let Some(Build::Embedded(EmbeddedSource::Package(package))) = &pkg.build {
            let parent = self
                .read_components_from_storage(&(&package.ident).try_into()?)
                .await?;
            // XXX Do embedded packages always/only have the Run component?
            // XXX Supplying a "random" digest here.
            if let Some((_, digest)) = parent.into_iter().next() {
                return Ok(HashMap::from([(Component::Run, digest)]));
            } else {
                return Ok(HashMap::default());
            }
        }
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
