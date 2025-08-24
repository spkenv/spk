// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Handles the setup and initialization of runtime environments

pub mod live_layer;
mod config_nu;
mod env_nu;
#[cfg(unix)]
pub mod overlayfs;
pub mod spec_api_version;
#[cfg(unix)]
mod startup_csh;
#[cfg(windows)]
mod startup_ps;
#[cfg(unix)]
mod startup_sh;
mod storage;
#[cfg(windows)]
pub mod winfsp;

pub use live_layer::{BindMount, LiveLayer, LiveLayerContents};
#[cfg(unix)]
pub use overlayfs::is_removed_entry;
pub use spec_api_version::SpecApiVersion;
pub use storage::{
    Author,
    Config,
    Data,
    EnvKeyValue,
    KeyValuePair,
    KeyValuePairBuf,
    MountBackend,
    OwnedRuntime,
    Runtime,
    STARTUP_FILES_LOCATION,
    Status,
    Storage,
    makedirs_with_perms,
};
#[cfg(windows)]
pub use winfsp::is_removed_entry;
