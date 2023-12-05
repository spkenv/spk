// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod blob;
mod error;
mod layer;
mod manifest;
pub mod payload;
mod platform;
mod repository;
mod tag;

mod config;
pub mod fallback;
pub mod fs;
mod handle;
pub mod pinned;
pub mod prelude;
pub mod proxy;
pub mod rpc;
pub mod tar;

use std::sync::Arc;

pub use blob::BlobStorage;
use chrono::{DateTime, Utc};
pub use error::OpenRepositoryError;
pub use layer::LayerStorage;
pub use manifest::ManifestStorage;
pub use payload::PayloadStorage;
pub use platform::PlatformStorage;
pub use proxy::{Config, ProxyRepository};
pub use repository::{LocalRepository, Repository};
pub use tag::{EntryType, TagStorage};

pub use self::config::{FromConfig, FromUrl, OpenRepositoryResult};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum RepositoryHandle {
    FS(fs::FsRepository),
    Tar(tar::TarRepository),
    Rpc(rpc::RpcRepository),
    FallbackProxy(Box<fallback::FallbackProxy>),
    Proxy(Box<proxy::ProxyRepository>),
    Pinned(Box<pinned::PinnedRepository<RepositoryHandle>>),
}

impl RepositoryHandle {
    /// Pin this repository to a specific date time, limiting
    /// all results to that instant and before.
    ///
    /// If this repository is already pinned, this function
    /// CAN move the pin farther into the future than it was
    /// before. In other words, pinned repositories are never
    /// nested via this function call.
    pub fn into_pinned(self, time: DateTime<Utc>) -> Self {
        match self {
            RepositoryHandle::Pinned(pinned) => Self::Pinned(Box::new(
                pinned::PinnedRepository::new(Arc::clone(pinned.inner()), time),
            )),
            _ => Self::Pinned(Box::new(pinned::PinnedRepository::new(
                Arc::new(self),
                time,
            ))),
        }
    }

    /// Make a pinned version of this repository at a specific date time,
    /// limiting all results to that instant and before.
    ///
    /// If this repository is already pinned, this function
    /// CAN move the pin farther into the future than it was
    /// before. In other words, pinned repositories are never
    /// nested via this function call.
    pub fn to_pinned(self: &Arc<Self>, time: DateTime<Utc>) -> Self {
        match &**self {
            RepositoryHandle::Pinned(pinned) => Self::Pinned(Box::new(
                pinned::PinnedRepository::new(Arc::clone(pinned.inner()), time),
            )),
            _ => Self::Pinned(Box::new(pinned::PinnedRepository::new(
                Arc::clone(self),
                time,
            ))),
        }
    }
}

impl RepositoryHandle {
    pub fn to_repo(self) -> Box<dyn Repository> {
        match self {
            Self::FS(repo) => Box::new(repo),
            Self::Tar(repo) => Box::new(repo),
            Self::Rpc(repo) => Box::new(repo),
            Self::FallbackProxy(repo) => repo,
            Self::Proxy(repo) => repo,
            Self::Pinned(repo) => repo,
        }
    }
}

impl std::ops::Deref for RepositoryHandle {
    type Target = dyn Repository;

    fn deref(&self) -> &Self::Target {
        match self {
            RepositoryHandle::FS(repo) => repo,
            RepositoryHandle::Tar(repo) => repo,
            RepositoryHandle::Rpc(repo) => repo,
            RepositoryHandle::FallbackProxy(repo) => &**repo,
            RepositoryHandle::Proxy(repo) => &**repo,
            RepositoryHandle::Pinned(repo) => &**repo,
        }
    }
}

impl std::ops::DerefMut for RepositoryHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            RepositoryHandle::FS(repo) => repo,
            RepositoryHandle::Tar(repo) => repo,
            RepositoryHandle::Rpc(repo) => repo,
            RepositoryHandle::FallbackProxy(repo) => &mut **repo,
            RepositoryHandle::Proxy(repo) => &mut **repo,
            RepositoryHandle::Pinned(repo) => &mut **repo,
        }
    }
}

impl From<fs::FsRepository> for RepositoryHandle {
    fn from(repo: fs::FsRepository) -> Self {
        RepositoryHandle::FS(repo)
    }
}

impl From<fs::OpenFsRepository> for RepositoryHandle {
    fn from(repo: fs::OpenFsRepository) -> Self {
        RepositoryHandle::FS(repo.into())
    }
}

impl From<Arc<fs::OpenFsRepository>> for RepositoryHandle {
    fn from(repo: Arc<fs::OpenFsRepository>) -> Self {
        RepositoryHandle::FS(repo.into())
    }
}

impl From<tar::TarRepository> for RepositoryHandle {
    fn from(repo: tar::TarRepository) -> Self {
        RepositoryHandle::Tar(repo)
    }
}

impl From<rpc::RpcRepository> for RepositoryHandle {
    fn from(repo: rpc::RpcRepository) -> Self {
        RepositoryHandle::Rpc(repo)
    }
}

impl From<fallback::FallbackProxy> for RepositoryHandle {
    fn from(repo: fallback::FallbackProxy) -> Self {
        RepositoryHandle::FallbackProxy(Box::new(repo))
    }
}

impl From<proxy::ProxyRepository> for RepositoryHandle {
    fn from(repo: proxy::ProxyRepository) -> Self {
        RepositoryHandle::Proxy(Box::new(repo))
    }
}

impl From<Box<pinned::PinnedRepository<RepositoryHandle>>> for RepositoryHandle {
    fn from(repo: Box<pinned::PinnedRepository<RepositoryHandle>>) -> Self {
        RepositoryHandle::Pinned(repo)
    }
}
