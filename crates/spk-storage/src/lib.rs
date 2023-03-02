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
    local_repository,
    remote_repository,
    CachePolicy,
    Repository,
    RepositoryHandle,
    Storage,
};
