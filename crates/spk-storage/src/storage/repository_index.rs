// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf};
use spk_schema::foundation::version::Version;
use spk_schema::ident::VersionIdent;
use spk_schema::name::OptNameBuf;
use spk_schema::{BuildIdent, Spec};

use crate::Result;
use crate::storage::FlatBufferRepoIndex;

/// Index read operations for a repository index
pub trait RepositoryIndex: Sync {
    /// To help the resolvo solver
    fn get_global_var_values(&self) -> HashMap<OptNameBuf, HashSet<String>>;

    /// For solving, closely related to the Repository trait methods of the same names
    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>>;

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>>;

    async fn list_package_builds(&self, pkg: &VersionIdent) -> Result<Vec<BuildIdent>>;

    async fn list_build_components(&self, pkg: &BuildIdent) -> Result<Vec<Component>>;

    async fn is_build_deprecated(&self, build: &BuildIdent) -> Result<bool>;

    /// Returns a valid package build spec for the given package build
    /// ident from the index.
    fn get_package_build_spec(&self, pkg: &BuildIdent) -> Result<Arc<Spec>>;
}

/// Index creation and updating operations for a repository index
#[async_trait::async_trait]
pub trait RepositoryIndexMut {
    /// Generate the index for the given repo and store it for later use
    #[allow(clippy::ptr_arg)]
    async fn index_repo(repos: &Vec<(String, crate::RepositoryHandle)>) -> miette::Result<()>;

    /// Update the existing index for the given package versions. The
    /// package versions to update will have their data gathered from
    /// the repository, rather than the current index. This is useful
    /// when a package has been published to a repo to add it to the
    /// index without generating the entire index from scratch.
    async fn update_packages(
        &self,
        repo: &crate::RepositoryHandle,
        package_versions: &[VersionIdent],
    ) -> miette::Result<()>;
}

/// Type for wrapping different kinds of indexes
#[derive(Debug, Clone)]
pub enum RepoIndex {
    Flat(FlatBufferRepoIndex),
}

impl RepositoryIndex for RepoIndex {
    fn get_global_var_values(&self) -> HashMap<OptNameBuf, HashSet<String>> {
        match self {
            RepoIndex::Flat(i) => i.get_global_var_values(),
        }
    }

    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>> {
        match self {
            RepoIndex::Flat(i) => i.list_packages().await,
        }
    }

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>> {
        match self {
            RepoIndex::Flat(i) => i.list_package_versions(name).await,
        }
    }

    async fn list_package_builds(&self, pkg: &VersionIdent) -> Result<Vec<BuildIdent>> {
        match self {
            RepoIndex::Flat(i) => i.list_package_builds(pkg).await,
        }
    }

    async fn list_build_components(&self, pkg: &BuildIdent) -> Result<Vec<Component>> {
        match self {
            RepoIndex::Flat(i) => i.list_build_components(pkg).await,
        }
    }

    async fn is_build_deprecated(&self, build: &BuildIdent) -> Result<bool> {
        match self {
            RepoIndex::Flat(i) => i.is_build_deprecated(build).await,
        }
    }

    fn get_package_build_spec(&self, pkg: &BuildIdent) -> Result<Arc<Spec>> {
        match self {
            RepoIndex::Flat(i) => i.get_package_build_spec(pkg),
        }
    }
}
