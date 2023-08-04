// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Handles the setup and initialization of runtime environments

#[cfg(unix)]
pub mod overlayfs;
mod startup_csh;
mod startup_sh;
mod storage;
#[cfg(windows)]
pub mod winfsp;

#[cfg(unix)]
pub use overlayfs::is_removed_entry;
pub use storage::{
    makedirs_with_perms,
    Author,
    Config,
    Data,
    MountBackend,
    OwnedRuntime,
    Runtime,
    Status,
    Storage,
    STARTUP_FILES_LOCATION,
};
#[cfg(windows)]
pub use winfsp::is_removed_entry;
