// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Functions related to the monitoring of an active spfs runtime on windows

use std::collections::HashMap;
use std::path::PathBuf;

use super::runtime;
use crate::Result;

pub const SPFS_MONITOR_FOREGROUND_LOGGING_VAR: &str = "SPFS_MONITOR_FOREGROUND_LOGGING";

/// Run an spfs monitor for the provided runtime
///
/// The monitor command will spawn but immediately fail
/// if there is already a monitor registered to this runtime
pub fn spawn_monitor_for_runtime(_rt: &runtime::Runtime) -> Result<tokio::process::Child> {
    todo!()
}

/// When provided an active runtime, wait until all contained processes exit
///
/// This is a privileged operation that may fail with a permission
/// issue if the calling process is not root or CAP_NET_ADMIN
pub async fn wait_for_empty_runtime(_rt: &runtime::Runtime) -> Result<()> {
    todo!()
}

/// Identify the mount namespace of the provided process id.
///
/// Return None if the pid is not found.
pub async fn identify_mount_namespace_of_process(_pid: u32) -> Result<Option<std::path::PathBuf>> {
    todo!()
}

/// Return an inventory of all known pids and their mount namespaces.
pub async fn find_processes_and_mount_namespaces() -> Result<HashMap<u32, Option<PathBuf>>> {
    todo!()
}
