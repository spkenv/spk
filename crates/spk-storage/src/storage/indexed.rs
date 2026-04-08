// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use arc_swap::ArcSwap;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName};
use spk_schema::foundation::version::Version;
use spk_schema::ident::VersionIdent;
use spk_schema::ident_build::EmbeddedSource;
use spk_schema::name::OptNameBuf;
use spk_schema::{BuildIdent, Spec, SpecRecipe};

use super::repository::{PublishPolicy, Repository, Storage};
use crate::storage::{FLATBUFFER_INDEX, FlatBufferRepoIndex, RepoIndex, RepositoryIndex};
use crate::{Error, Result};

/// A spk repository that wraps another repository with that
/// repository's index. Operations that solvers need will use the
/// index and typically be faster, other operations, especially
/// writes, will pass through to the underlying wrapped repository.
#[derive(Debug)]
pub struct IndexedRepository {
    /// The index of the wrapped repo
    index: ArcSwap<RepoIndex>,
    /// The underlying repo that has an index
    wrapped_repo: Arc<crate::RepositoryHandle>,
    /// For automated tests
    update_index_after_any_publish: bool,
}

impl Clone for IndexedRepository {
    fn clone(&self) -> Self {
        Self {
            index: ArcSwap::new(self.index.load_full()),
            wrapped_repo: self.wrapped_repo.clone(),
            update_index_after_any_publish: self.update_index_after_any_publish,
        }
    }
}

impl IndexedRepository {
    /// Get the name of the kind of index from spk's config. This is
    /// used to work out what kind of index to load or save.
    fn get_index_kind_from_config() -> Result<String> {
        let config = spk_config::get_config()?;

        let index_kind = if config.solver.indexes.kind != String::default() {
            config.solver.indexes.kind.clone()
        } else {
            String::from(FLATBUFFER_INDEX)
        };

        tracing::debug!("Index kind from config: '{index_kind}'");
        Ok(index_kind)
    }

    /// Set whether to update the internal index after any publish or
    /// write operation on this repository. This is meant for use by
    /// automated testing. Updating the index continuously may be costly.
    pub fn set_update_index_after_any_publish(&mut self, value: bool) {
        self.update_index_after_any_publish = value
    }

    /// Create an IndexedRepository for the given repo by loading the
    /// index from, and for, that repo.
    pub async fn load_from_repo(
        repo_to_wrap: Arc<crate::RepositoryHandle>,
    ) -> Result<IndexedRepository> {
        let index_kind = IndexedRepository::get_index_kind_from_config()?;

        let index = match index_kind.as_ref() {
            FLATBUFFER_INDEX => {
                tracing::debug!("Flatbuffer index selected");

                match FlatBufferRepoIndex::from_repo_file(&repo_to_wrap).await {
                    Ok(i) => RepoIndex::Flat(i),
                    Err(err) => {
                        return Err(Error::IndexFailedToLoad(err.to_string()));
                    }
                }
            }
            _ => {
                return Err(Error::IndexUnknownKind(
                    index_kind,
                    "load from file".to_string(),
                ));
            }
        };

        Ok(IndexedRepository {
            index: ArcSwap::new(Arc::new(index)),
            wrapped_repo: repo_to_wrap,
            update_index_after_any_publish: false,
        })
    }

    /// Internal helper method for generating an index in memory.
    async fn generate_in_memory_index_from_repo(
        repo_to_wrap: &Arc<crate::RepositoryHandle>,
    ) -> Result<RepoIndex> {
        let index_kind = IndexedRepository::get_index_kind_from_config()?;

        match index_kind.as_ref() {
            FLATBUFFER_INDEX => {
                tracing::debug!("Flatbuffer index selected");

                match FlatBufferRepoIndex::from_repo_in_memory(repo_to_wrap).await {
                    Ok(i) => Ok(RepoIndex::Flat(i)),
                    Err(err) => Err(Error::IndexFailedToGenerate(err.to_string())),
                }
            }
            _ => Err(Error::IndexUnknownKind(
                index_kind,
                "create in memory".to_string(),
            )),
        }
    }

    /// Create an IndexedRepository from the given repo by generating
    /// an in-memory index from the repository's data. This may take
    /// some time and does not save the index anywhere else.
    pub async fn generate_from_repo(
        repo_to_wrap: Arc<crate::RepositoryHandle>,
    ) -> Result<IndexedRepository> {
        let index = IndexedRepository::generate_in_memory_index_from_repo(&repo_to_wrap).await?;

        Ok(IndexedRepository {
            index: ArcSwap::new(Arc::new(index)),
            wrapped_repo: repo_to_wrap,
            update_index_after_any_publish: false,
        })
    }

    /// Rebuild the index in memory from the underlying repo.
    async fn rebuild_internal_index(&self) -> Result<()> {
        let new_index =
            IndexedRepository::generate_in_memory_index_from_repo(&self.wrapped_repo).await?;

        self.index.store(Arc::new(new_index));
        Ok(())
    }

    /// Returns a mapping of all the global vars and their values from
    /// all the builds in the repo. This is used to prime the resolvo solver.
    pub fn get_global_var_values(&self) -> HashMap<OptNameBuf, HashSet<String>> {
        self.index.load().get_global_var_values()
    }
}

impl std::hash::Hash for IndexedRepository {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Pass through to the wrapped repo
        self.wrapped_repo.address().hash(state);
    }
}

impl Ord for IndexedRepository {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Pass through to the wrapped repo
        self.wrapped_repo
            .address()
            .cmp(other.wrapped_repo.address())
    }
}

impl PartialOrd for IndexedRepository {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for IndexedRepository {
    fn eq(&self, other: &Self) -> bool {
        // Pass through to the wrapped repo
        self.wrapped_repo.address() == other.wrapped_repo.address()
    }
}

impl Eq for IndexedRepository {}

#[async_trait::async_trait]
impl Storage for IndexedRepository {
    type Recipe = SpecRecipe;
    type Package = Spec;

    async fn get_concrete_package_builds(&self, pkg: &VersionIdent) -> Result<HashSet<BuildIdent>> {
        // This almost identical to embedded builds method below, but
        // with the filter_map function swapped around.
        let all_builds = self.index.load().list_package_builds(pkg).await?;
        Ok(all_builds
            .into_iter()
            .filter_map(|b| if b.is_embedded() { None } else { Some(b) })
            .collect())
    }

    async fn get_embedded_package_builds(&self, pkg: &VersionIdent) -> Result<HashSet<BuildIdent>> {
        // This almost identical to concrete builds method above, but
        // with the filter_map functions swapped around.
        let all_builds = self.index.load().list_package_builds(pkg).await?;
        Ok(all_builds
            .into_iter()
            .filter_map(|b| if b.is_embedded() { Some(b) } else { None })
            .collect())
    }

    async fn publish_embed_stub_to_storage(&self, spec: &Self::Package) -> Result<()> {
        // Pass through to the wrapped repo to do the update
        self.wrapped_repo
            .publish_embed_stub_to_storage(spec)
            .await?;

        // Rebuild the index only if enabled
        if self.update_index_after_any_publish {
            self.rebuild_internal_index().await?;
        }

        Ok(())
    }

    async fn publish_package_to_storage(
        &self,
        package: &<Self::Recipe as spk_schema::Recipe>::Output,
        components: &HashMap<Component, spfs::encoding::Digest>,
    ) -> Result<()> {
        // Pass thru to the wrapped repo to do the update
        self.wrapped_repo
            .publish_package_to_storage(package, components)
            .await?;

        // Rebuild the index only if enabled
        if self.update_index_after_any_publish {
            self.rebuild_internal_index().await?;
        }

        Ok(())
    }

    async fn publish_recipe_to_storage(
        &self,
        spec: &Self::Recipe,
        publish_policy: PublishPolicy,
    ) -> Result<()> {
        // Pass thru to the wrapped repo to do the update
        self.wrapped_repo
            .publish_recipe_to_storage(spec, publish_policy)
            .await?;

        // Rebuild the index only if enabled
        if self.update_index_after_any_publish {
            self.rebuild_internal_index().await?;
        }

        Ok(())
    }

    async fn read_components_from_storage(
        &self,
        pkg: &BuildIdent,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        // Pass through to the wrapped repo
        self.wrapped_repo.read_components_from_storage(pkg).await
    }

    async fn read_package_from_storage(
        &self,
        pkg: &BuildIdent,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>> {
        // TODO: remove or put back, this commented out code block
        // once deprecation and indexing is decided on.
        // if self.is_build_deprecated(pkg).await? {
        //     // Deprecated builds are stored as partial entries in the
        //     // index so they have to be read from the underlying repo.
        //     self.wrapped_repo.read_package_from_storage(pkg).await
        // } else {
        self.index.load().get_package_build_spec(pkg)
        //}
    }

    async fn remove_embed_stub_from_storage(&self, pkg: &BuildIdent) -> Result<()> {
        // Pass through to the wrapped repo to do the update
        self.wrapped_repo
            .remove_embed_stub_from_storage(pkg)
            .await?;

        // Rebuild the index only if enabled
        if self.update_index_after_any_publish {
            self.rebuild_internal_index().await?;
        }

        Ok(())
    }

    async fn remove_package_from_storage(&self, pkg: &BuildIdent) -> Result<()> {
        // Pass through to the wrapped repo to do the update
        self.wrapped_repo.remove_package_from_storage(pkg).await?;

        // Rebuild the index only if enabled
        if self.update_index_after_any_publish {
            self.rebuild_internal_index().await?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl Repository for IndexedRepository {
    fn address(&self) -> &url::Url {
        // Pass through to the wrapped repo
        self.wrapped_repo.address()
    }

    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>> {
        self.index.load().list_packages().await
    }

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>> {
        self.index.load().list_package_versions(name).await
    }

    async fn list_package_builds(&self, pkg: &VersionIdent) -> Result<Vec<BuildIdent>> {
        self.index.load().list_package_builds(pkg).await
    }

    async fn list_build_components(&self, pkg: &BuildIdent) -> Result<Vec<Component>> {
        self.index.load().list_build_components(pkg).await
    }

    async fn is_build_deprecated(&self, build: &BuildIdent) -> Result<bool> {
        self.index.load().is_build_deprecated(build).await
    }

    fn name(&self) -> &RepositoryName {
        // Pass through to the wrapped repo
        self.wrapped_repo.name()
    }

    async fn read_embed_stub(&self, pkg: &BuildIdent) -> Result<Arc<Self::Package>> {
        match pkg.build() {
            Build::Embedded(EmbeddedSource::Package { .. }) => {
                // Allow embedded stubs to be read as a "package"
            }
            _ => {
                return Err(format!("Cannot read this ident as an embed stub: {pkg}").into());
            }
        };

        if self.is_build_deprecated(pkg).await? {
            // Deprecated builds are stored as partial entries in the
            // index so they have to be read from the underlying repo.
            self.wrapped_repo.read_package_from_storage(pkg).await
        } else {
            self.index.load().get_package_build_spec(pkg)
        }
    }

    async fn read_recipe(&self, pkg: &VersionIdent) -> Result<Arc<Self::Recipe>> {
        // Pass through to the wrapped repo
        self.wrapped_repo.read_recipe(pkg).await
    }

    async fn remove_recipe(&self, pkg: &VersionIdent) -> Result<()> {
        // Pass through to the wrapped repo to do the update
        self.wrapped_repo.remove_recipe(pkg).await?;

        // Rebuild the index only if enabled
        if self.update_index_after_any_publish {
            self.rebuild_internal_index().await?;
        }

        Ok(())
    }
}
