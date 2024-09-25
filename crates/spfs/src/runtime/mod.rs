// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Handles the setup and initialization of runtime environments

pub mod live_layer;
#[cfg(unix)]
pub mod overlayfs;
#[cfg(unix)]
mod startup_csh;
#[cfg(windows)]
mod startup_ps;
#[cfg(unix)]
mod startup_sh;
mod storage;
#[cfg(windows)]
pub mod winfsp;

pub use live_layer::{BindMount, LiveLayer, LiveLayerContents, SpfsFileApiVersion};
#[cfg(unix)]
pub use overlayfs::is_removed_entry;
pub use storage::{
    makedirs_with_perms,
    Author,
    Config,
    Data,
    KeyValuePair,
    KeyValuePairBuf,
    MountBackend,
    OwnedRuntime,
    Runtime,
    Status,
    Storage,
    STARTUP_FILES_LOCATION,
};
#[cfg(windows)]
pub use winfsp::is_removed_entry;
