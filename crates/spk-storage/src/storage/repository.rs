// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName};
use spk_schema::foundation::version::Version;
use spk_schema::ident_build::{Build, EmbeddedSource, InvalidBuildError};
use spk_schema::{BuildIdent, Deprecate, Package, PackageMut, VersionIdent};

use self::internal::RepositoryExt;
use crate::{Error, Result};

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
    type Recipe: spk_schema::Recipe<Output = Self::Package>;
    type Package: Package<Package = Self::Package>;

    /// Return the set of concrete builds for the given package name and version.
    ///
    /// This method should not return any embedded package stub builds.
    async fn get_concrete_package_builds(&self, pkg: &VersionIdent) -> Result<HashSet<BuildIdent>>;

    /// Return the set of embedded stub builds for the given package name and version.
    ///
    /// This method should only return embedded package stub builds.
    async fn get_embedded_package_builds(&self, pkg: &VersionIdent) -> Result<HashSet<BuildIdent>>;

    /// Publish an embed stub to this repository.
    ///
    /// An embed stub represents an embedded portion of some other package.
    /// The stub exists to advertise the existence of the embedded package.
    async fn publish_embed_stub_to_storage(&self, spec: &Self::Package) -> Result<()>;

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
        pkg: &BuildIdent,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>>;

    /// Read package information for a specific version and build.
    ///
    /// # Errors:
    /// - PackageNotFoundError: If the package, version, or build does not exist
    async fn read_package_from_storage(
        &self,
        pkg: &BuildIdent,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>>;

    /// Remove an embed stub from this repository.
    ///
    /// The given package identifier must identify a [`Build::Embedded`].
    async fn remove_embed_stub_from_storage(&self, pkg: &BuildIdent) -> Result<()>;

    /// Remove a package from this repository.
    async fn remove_package_from_storage(&self, pkg: &BuildIdent) -> Result<()>;
}

pub(in crate::storage) mod internal {
    use std::collections::{BTreeSet, HashMap};

    use spk_schema::foundation::ident_build::{Build, EmbeddedSource, EmbeddedSourcePackage};
    use spk_schema::foundation::ident_component::Component;
    use spk_schema::{Deprecate, DeprecateMut, Package, PackageMut, Recipe};

    use crate::{with_cache_policy, CachePolicy, Result};

    /// Reusable methods for [`super::Repository`] that are not intended to be
    /// part of its public interface.
    #[async_trait::async_trait]
    pub trait RepositoryExt: super::Repository {
        /// Add a [`Package`] to a repository to represent an embedded package.
        ///
        /// This spec is a link back to the package that embeds it, and allows
        /// the repository to advertise its existence.
        async fn create_embedded_stub_for_spec(
            &self,
            spec_for_parent: &Self::Package,
            spec_for_embedded_pkg: &Self::Package,
            components_that_embed_this_pkg: BTreeSet<Component>,
        ) -> Result<()>
        where
            Self::Package: PackageMut,
        {
            let mut spec_for_embedded_pkg = spec_for_embedded_pkg.clone();
            spec_for_embedded_pkg.set_build(Build::Embedded(EmbeddedSource::Package(Box::new(
                EmbeddedSourcePackage {
                    ident: spec_for_parent.ident().into(),
                    components: components_that_embed_this_pkg,
                },
            ))));
            spec_for_embedded_pkg.set_deprecated(spec_for_parent.is_deprecated())?;
            self.publish_embed_stub_to_storage(&spec_for_embedded_pkg)
                .await
        }

        /// Get all the embedded packages described by a [`Package`] and
        /// return what [`Component`]s are providing each one.
        fn get_embedded_providers(
            &self,
            package: &<Self::Recipe as Recipe>::Output,
        ) -> Result<HashMap<<Self as super::Storage>::Package, BTreeSet<Component>>> {
            let mut embedded_providers = HashMap::new();
            for (embed, component) in package.embedded_as_packages()?.into_iter() {
                // "top level" embedded as assumed to be provided by the "run"
                // component.
                (*embedded_providers
                    .entry(embed)
                    .or_insert_with(BTreeSet::new))
                .insert(component.unwrap_or(Component::Run));
            }
            Ok(embedded_providers)
        }

        /// Remove the [`Package`] from a repository that represents the
        /// embedded package of some other package.
        ///
        /// The stub should be removed when the package that had been embedding
        /// the package is removed or modified such that it no longer embeds
        /// this package.
        async fn remove_embedded_stub_for_spec(
            &self,
            spec_for_parent: &Self::Package,
            spec_for_embedded_pkg: &Self::Package,
            components_that_embed_this_pkg: BTreeSet<Component>,
        ) -> Result<()> {
            let spec_for_embedded_pkg =
                spec_for_embedded_pkg
                    .ident()
                    .with_build(Build::Embedded(EmbeddedSource::Package(Box::new(
                        EmbeddedSourcePackage {
                            ident: spec_for_parent.ident().into(),
                            components: components_that_embed_this_pkg,
                        },
                    ))));
            self.remove_embed_stub_from_storage(&spec_for_embedded_pkg)
                .await?;

            // If this was the last stub and there are no other builds, remove
            // the "version spec".
            if let Ok(builds) = with_cache_policy!(self, CachePolicy::BypassCache, {
                self.list_package_builds(spec_for_embedded_pkg.as_version())
            })
            .await
            {
                if builds.is_empty() {
                    if let Err(err) = self.remove_recipe(spec_for_embedded_pkg.as_version()).await {
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

    /// Return the active highest version number available for the
    /// named package. Versions with all their builds deprecated are
    /// excluded.
    async fn highest_package_version(&self, name: &PkgName) -> Result<Option<Arc<Version>>> {
        let versions: Arc<Vec<Arc<Version>>> = self.list_package_versions(name).await?;
        // Not all repo implementations will return a sorted list from
        // list_package_versions, and this needs them reverse sorted.
        let mut sorted_versions = (*versions).clone();
        sorted_versions.sort_by(|a, b| b.cmp(a));

        for version in sorted_versions.iter() {
            // Check the version's builds. It must have one active,
            // non-deprecated build for the version to also be active.
            let ident = VersionIdent::new(name.to_owned(), (**version).clone());
            let builds = self.list_package_builds(&ident).await?;
            if builds.is_empty() {
                continue;
            }
            for build in builds {
                match self.read_package(&build).await {
                    Ok(spec) if !spec.is_deprecated() => {
                        // Found an active build for this version, so
                        // it's the highest version
                        return Ok(Some(Arc::clone(version)));
                    }
                    Ok(_) => {}
                    Err(err) => return Err(err),
                }
            }
        }
        // There is no active version of the package in this
        // repository.
        Ok(None)
    }

    /// Return the set of builds for the given package name and version.
    async fn list_package_builds(&self, pkg: &VersionIdent) -> Result<Vec<BuildIdent>> {
        // Note: This isn't cached. Neither get_concrete_package_builds() nor
        // get_embedded_package_builds() are cached. But the underlying
        // ls_tags() calls they both make are cached.
        // TODO: could be worth caching, depending on the average
        // builds per version.
        let concrete_builds = self.get_concrete_package_builds(pkg);
        let embedded_builds = self.get_embedded_package_builds(pkg);
        let (mut concrete, embedded) = tokio::try_join!(concrete_builds, embedded_builds)?;
        concrete.extend(embedded.into_iter());
        Ok(concrete.into_iter().collect())
    }

    /// Returns the set of components published for a package build
    async fn list_build_components(&self, pkg: &BuildIdent) -> Result<Vec<Component>>;

    /// Return the repository's name, as in "local" or its name in the config file.
    fn name(&self) -> &RepositoryName;

    /// Read an embed stub.
    ///
    /// # Errors:
    /// - PackageNotFoundError: If the package, or version does not exist
    async fn read_embed_stub(&self, pkg: &BuildIdent) -> Result<Arc<Self::Package>>;

    /// Read a package recipe for the given package, and version.
    ///
    /// # Errors:
    /// - PackageNotFoundError: If the package, or version does not exist
    async fn read_recipe(&self, pkg: &VersionIdent) -> Result<Arc<Self::Recipe>>;

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
    async fn remove_recipe(&self, pkg: &VersionIdent) -> Result<()>;

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
        pkg: &BuildIdent,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>> {
        if pkg.build().is_embed_stub() {
            self.read_embed_stub(pkg).await
        } else {
            self.read_package_from_storage(pkg).await
        }
    }

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
        Self::Package: PackageMut,
    {
        if package.ident().build().is_embedded() {
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
        Self::Package: PackageMut,
    {
        // Read the contents of the existing spec, if any, before it is
        // overwritten.
        let original_spec = if package.ident().can_embed() {
            Some(
                crate::with_cache_policy!(self, CachePolicy::BypassCache, {
                    self.read_package(package.ident())
                })
                .await,
            )
        } else {
            None
        };

        let components = self.read_components(package.ident()).await?;
        self.publish_package_to_storage(package, &components)
            .await?;

        // Changes that affect embedded stubs:
        // - change in deprecation status
        // - adding/removing embedded packages
        if let Some(Ok(original_spec)) = original_spec {
            let original_embedded_providers = self.get_embedded_providers(&original_spec)?;
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
                let original_keys: HashSet<&Self::Package> =
                    original_embedded_providers.keys().collect();
                let new_keys: HashSet<&Self::Package> = new_embedded_providers.keys().collect();

                // First deal with embeds that appeared or disappeared.
                for added_or_removed_spec in original_keys.symmetric_difference(&new_keys) {
                    if original_keys.contains(added_or_removed_spec) {
                        // This embed was removed
                        if let Some(components) =
                            original_embedded_providers.get(*added_or_removed_spec)
                        {
                            self.remove_embedded_stub_for_spec(
                                package,
                                added_or_removed_spec,
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
                                added_or_removed_spec,
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
                            returning_spec,
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
    async fn remove_package(&self, pkg: &BuildIdent) -> Result<()> {
        // Attempt to find and remove any related embedded package stubs.
        if let Ok(spec) = self.read_package(pkg).await {
            if spec.ident().can_embed() {
                let embedded_providers = self.get_embedded_providers(&spec)?;

                for (embed, components) in embedded_providers.into_iter() {
                    self.remove_embedded_stub_for_spec(&spec, &embed, components)
                        .await?
                }
            }
        }

        self.remove_package_from_storage(pkg).await
    }

    /// Identify the payloads for this identified package's components.
    async fn read_components(
        &self,
        pkg: &BuildIdent,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        if let Build::Embedded(EmbeddedSource::Package(_package)) = pkg.build() {
            // An embedded package's components are only accessible
            // via its package spec
            let embedded_spec = self.read_package(pkg).await?;
            let components = embedded_spec
                .components()
                .iter()
                .map(|c| (c.name.clone(), spfs::encoding::EMPTY_DIGEST.into()))
                .collect::<HashMap<Component, spfs::encoding::Digest>>();

            return Ok(components);
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
