// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! The spfs filesystem is the main point of interaction with the system.
//!
//! This module contains logic for setting up and operating the filesystem
//! for different systems and contexts.

mod file_system;
mod mount_strategy;
#[cfg(target_os = "linux")]
pub mod overlayfs;

pub use file_system::FileSystem;
pub use mount_strategy::MountStrategy;

pub mod prelude {
    pub use super::FileSystem;
}
