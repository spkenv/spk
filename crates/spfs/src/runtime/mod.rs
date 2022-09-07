// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Handles the setup and initialization of runtime environments

mod csh_exp;
mod startup_csh;
mod startup_sh;
mod storage;
#[cfg(feature = "runtime-compat-0.33")]
pub mod storage_033;

pub use storage::{
    makedirs_with_perms, Author, Config, Data, OwnedRuntime, Runtime, Status, Storage,
    STARTUP_FILES_LOCATION,
};
