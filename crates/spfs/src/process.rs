// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Process tracking and management

#[cfg(target_os = "macos")]
mod process_macos;

#[cfg(target_os = "macos")]
pub use process_macos::{
    ProcessError, ProcessWatcher, get_descendant_pids, get_parent_pid, get_parent_pid_for,
    get_parent_pids_macos, is_in_process_tree,
};
