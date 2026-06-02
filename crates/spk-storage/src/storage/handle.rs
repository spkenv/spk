// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use spk_schema::{Spec, SpecRecipe};
use variantly::Variantly;

use super::Repository;
use super::messaging::listen_to_index_status_until_updated;
use crate::{Error, Result};

type Handle = dyn Repository<Recipe = SpecRecipe, Package = Spec>;

#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Clone, Variantly)]
#[allow(clippy::large_enum_variant)]
pub enum RepositoryHandle {
    SPFS(super::SpfsRepository),
    Mem(super::MemRepository<SpecRecipe>),
    Runtime(super::RuntimeRepository),
    Indexed(super::IndexedRepository),
}

impl RepositoryHandle {
    /// Create a repository handle to an empty, in-memory repository
    pub fn new_mem() -> Self {
        Self::Mem(Default::default())
    }

    /// Create a repository handle to the active runtime repository
    ///
    /// This is the repository that holds packages which have been
    /// installed into the current spfs runtime.
    pub fn new_runtime() -> Self {
        Self::Runtime(Default::default())
    }

    pub fn to_repo(self) -> Box<Handle> {
        match self {
            Self::SPFS(repo) => Box::new(repo),
            Self::Mem(repo) => Box::new(repo),
            Self::Runtime(repo) => Box::new(repo),
            Self::Indexed(repo) => Box::new(repo),
        }
    }

    pub async fn index_location_path(&self) -> Result<PathBuf> {
        match self {
            Self::SPFS(spfs_repo) => spfs_repo.get_or_create_index_path().await,

            Self::Mem(mem_repo) => {
                // A mem repo does not have a usable location for
                // index files.
                Err(Error::IndexNoRepoLocationError(
                    mem_repo.name().to_string(),
                    "Spk Mem".to_string(),
                ))
            }

            Self::Runtime(runtime_repo) => {
                // A spk runtime repo does not have a usable location
                // index files.
                Err(Error::IndexNoRepoLocationError(
                    runtime_repo.name().to_string(),
                    "Spk Runtime".to_string(),
                ))
            }

            Self::Indexed(indexed_repo) => {
                // Indexed repositories store their index data based
                // on the repo they wrap, so use the underlying repo's
                // location. This is mildly recursive because the
                // wrapped repo is a spk RepositoryHandle.
                Box::pin(indexed_repo.wrapped_repo_index_location_path()).await
            }
        }
    }

    /// Clear any internal caches the repository has
    pub fn clear_caches(&self) {
        match self {
            Self::SPFS(spfs_repo) => spfs_repo.invalidate_caches(),
            _ => {
                // The other kinds of repository do not have and caches
            }
        }
    }

    /// Wait for the index associated with this repo, if there is one,
    /// to be updated. This is used by package changing operations
    /// (publish, remove, un/deprecate) to wait until the index has
    /// been updated with their changes before finishing.
    pub async fn wait_for_index_to_update(&self, update_time: &DateTime<Utc>) -> Result<()> {
        listen_to_index_status_until_updated(self, update_time).await
    }
}

impl std::ops::Deref for RepositoryHandle {
    type Target = Handle;

    fn deref(&self) -> &Self::Target {
        match self {
            RepositoryHandle::SPFS(repo) => repo,
            RepositoryHandle::Mem(repo) => repo,
            RepositoryHandle::Runtime(repo) => repo,
            RepositoryHandle::Indexed(repo) => repo,
        }
    }
}

impl std::ops::DerefMut for RepositoryHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            RepositoryHandle::SPFS(repo) => repo,
            RepositoryHandle::Mem(repo) => repo,
            RepositoryHandle::Runtime(repo) => repo,
            RepositoryHandle::Indexed(repo) => repo,
        }
    }
}

impl From<super::SpfsRepository> for RepositoryHandle {
    fn from(repo: super::SpfsRepository) -> Self {
        RepositoryHandle::SPFS(repo)
    }
}

impl From<super::MemRepository<SpecRecipe>> for RepositoryHandle {
    fn from(repo: super::MemRepository<SpecRecipe>) -> Self {
        RepositoryHandle::Mem(repo)
    }
}

impl From<super::RuntimeRepository> for RepositoryHandle {
    fn from(repo: super::RuntimeRepository) -> Self {
        RepositoryHandle::Runtime(repo)
    }
}

impl From<super::IndexedRepository> for RepositoryHandle {
    fn from(repo: super::IndexedRepository) -> Self {
        RepositoryHandle::Indexed(repo)
    }
}
