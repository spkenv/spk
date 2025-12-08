// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Configuration for macOS FUSE filesystem

use std::collections::HashSet;
use std::path::PathBuf;

use fuser::MountOption;

/// Options to configure the macFUSE filesystem and its behavior at runtime
#[derive(Debug, Clone)]
pub struct Config {
    /// The path where the filesystem should be mounted
    pub mountpoint: PathBuf,
    /// The permission bits for the root filesystem node
    pub root_mode: u32,
    /// The user id that should own all files and directories
    pub uid: nix::unistd::Uid,
    /// The group id that should own all files and directories
    pub gid: nix::unistd::Gid,
    /// Mount options to be used when setting up
    pub mount_options: HashSet<MountOption>,
    /// Remote repositories that can be read from.
    ///
    /// These are in addition to the local repository and
    /// are searched in order to find data.
    pub remotes: Vec<String>,
    /// Whether to have the tags in the secondary repos included in
    /// the lookup methods.
    pub include_secondary_tags: bool,
}

impl Default for Config {
    fn default() -> Self {
        let mut mount_options = HashSet::new();
        mount_options.insert(MountOption::RW);
        mount_options.insert(MountOption::NoDev);
        mount_options.insert(MountOption::NoSuid);
        mount_options.insert(MountOption::NoAtime);
        mount_options.insert(MountOption::Exec);
        // Note: AllowOther intentionally omitted for security
        // Note: FSName and Subtype added by service.rs

        Self {
            mountpoint: PathBuf::from("/spfs"),
            root_mode: 0o555,
            uid: nix::unistd::getuid(),
            gid: nix::unistd::getgid(),
            mount_options,
            remotes: Vec::new(),
            include_secondary_tags: false,
        }
    }
}
