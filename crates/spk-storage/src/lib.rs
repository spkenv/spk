// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod error;
pub mod fixtures;
mod storage;

pub use error::{Error, Result};
pub use storage::{
    export_package,
    find_path_providers,
    local_repository,
    pretty_print_filepath,
    remote_repository,
    CachePolicy,
    MemRepository,
    NameAndRepositoryWithTagStrategy,
    Repository,
    RepositoryHandle,
    RuntimeRepository,
    SpfsRepository,
    SpfsRepositoryHandle,
    Storage,
};
