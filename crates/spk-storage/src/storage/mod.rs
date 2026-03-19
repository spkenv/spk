// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod archive;
mod flatbuffer_index;
mod handle;
mod indexed;
mod mem;
mod repository;
mod repository_index;
mod runtime;
mod spfs;

pub use archive::export_package;
pub use flatbuffer_index::{FLATBUFFER_INDEX, FlatBufferRepoIndex};
pub use handle::RepositoryHandle;
pub use indexed::IndexedRepository;
pub use mem::MemRepository;
pub use repository::{CachePolicy, Repository, Storage};
pub use repository_index::{RepoIndex, RepositoryIndex, RepositoryIndexMut};
pub use runtime::{RuntimeRepository, find_path_providers, pretty_print_filepath};

pub use self::spfs::{
    NameAndRepository,
    SpfsRepository,
    inject_path_repo_into_spfs_config,
    local_repository,
    remote_repository,
};
