// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS process ancestry tracking using sysctl
//!
//! Re-exports from spfs::process module

pub use spfs::process::{
    ProcessError, ProcessWatcher, get_descendant_pids, get_parent_pid, get_parent_pids_macos,
    is_in_process_tree,
};
