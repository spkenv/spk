// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use super::Repository;
use crate::api;

type Handle = dyn Repository<Recipe = api::SpecRecipe>;

#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[allow(clippy::large_enum_variant)]
pub enum RepositoryHandle {
    SPFS(super::SPFSRepository),
    Mem(super::MemRepository<api::SpecRecipe>),
    Runtime(super::RuntimeRepository),
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

    pub fn is_spfs(&self) -> bool {
        matches!(self, Self::SPFS(_))
    }

    pub fn is_mem(&self) -> bool {
        matches!(self, Self::Mem(_))
    }

    pub fn is_runtime(&self) -> bool {
        matches!(self, Self::Runtime(_))
    }

    pub fn to_repo(self) -> Box<Handle> {
        match self {
            Self::SPFS(repo) => Box::new(repo),
            Self::Mem(repo) => Box::new(repo),
            Self::Runtime(repo) => Box::new(repo),
        }
    }
}

impl std::ops::Deref for RepositoryHandle {
    type Target = Handle;

    fn deref(&self) -> &Self::Target {
        match self {
            RepositoryHandle::SPFS(repo) => repo,
            RepositoryHandle::Mem(repo) => repo,
            RepositoryHandle::Runtime(repo) => repo,
        }
    }
}

impl std::ops::DerefMut for RepositoryHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            RepositoryHandle::SPFS(repo) => repo,
            RepositoryHandle::Mem(repo) => repo,
            RepositoryHandle::Runtime(repo) => repo,
        }
    }
}

impl From<super::SPFSRepository> for RepositoryHandle {
    fn from(repo: super::SPFSRepository) -> Self {
        RepositoryHandle::SPFS(repo)
    }
}

impl From<super::MemRepository<api::SpecRecipe>> for RepositoryHandle {
    fn from(repo: super::MemRepository<api::SpecRecipe>) -> Self {
        RepositoryHandle::Mem(repo)
    }
}

impl From<super::RuntimeRepository> for RepositoryHandle {
    fn from(repo: super::RuntimeRepository) -> Self {
        RepositoryHandle::Runtime(repo)
    }
}
