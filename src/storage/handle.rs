// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use super::Repository;

#[derive(Debug, Hash)]
pub enum RepositoryHandle {
    SPFS(super::SPFSRepository),
    Mem(super::MemRepository),
    Runtime(super::RuntimeRepository),
}

impl PartialEq for RepositoryHandle {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::SPFS(l0), Self::SPFS(r0)) => l0 == r0,
            (Self::Mem(l0), Self::Mem(r0)) => l0 == r0,
            (Self::Runtime(l0), Self::Runtime(r0)) => l0 == r0,
            _ => false,
        }
    }
}

impl Eq for RepositoryHandle {}

impl RepositoryHandle {
    /// The address of a repository identifies it's location and how
    /// it is being accessed.
    pub fn address(&self) -> url::Url {
        match self {
            Self::SPFS(repo) => repo.address(),
            Self::Mem(repo) => repo.address(),
            Self::Runtime(repo) => repo.address(),
        }
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

    pub fn to_repo(self) -> Box<dyn Repository> {
        match self {
            Self::SPFS(repo) => Box::new(repo),
            Self::Mem(repo) => Box::new(repo),
            Self::Runtime(repo) => Box::new(repo),
        }
    }
}

impl std::ops::Deref for RepositoryHandle {
    type Target = dyn Repository;

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

impl From<super::MemRepository> for RepositoryHandle {
    fn from(repo: super::MemRepository) -> Self {
        RepositoryHandle::Mem(repo)
    }
}

impl From<super::RuntimeRepository> for RepositoryHandle {
    fn from(repo: super::RuntimeRepository) -> Self {
        RepositoryHandle::Runtime(repo)
    }
}
