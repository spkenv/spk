// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod address;
mod blob;
mod error;
mod layer;
mod manifest;
pub mod payload;
mod platform;
mod repository;
mod tag;
mod tag_namespace;

mod config;
pub mod fallback;
pub mod fs;
mod handle;
pub mod pinned;
pub mod prelude;
pub mod proxy;
pub mod rpc;
pub mod tar;

pub use address::Address;
pub use blob::{BlobStorage, BlobStorageExt};
pub use error::OpenRepositoryError;
pub use handle::RepositoryHandle;
pub use layer::{LayerStorage, LayerStorageExt};
pub use manifest::ManifestStorage;
pub use payload::PayloadStorage;
pub use platform::{PlatformStorage, PlatformStorageExt};
pub use proxy::{Config, ProxyRepository};
pub use repository::{LocalRepository, Repository, RepositoryExt};
pub use tag::{EntryType, TagStorage, TagStorageMut};
pub use tag_namespace::{TagNamespace, TagNamespaceBuf, TAG_NAMESPACE_MARKER};

pub use self::config::{FromConfig, FromUrl, OpenRepositoryResult};
