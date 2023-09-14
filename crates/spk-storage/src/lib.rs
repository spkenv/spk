// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

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
    Repository,
    RepositoryHandle,
    RuntimeRepository,
    SpfsRepository,
    Storage,
};
