// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use super::Repository;

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum RepositoryHandle {
    SPFS(super::SPFSRepository),
    Mem(super::MemRepository),
    Runtime(super::RuntimeRepository),
}

impl RepositoryHandle {
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
