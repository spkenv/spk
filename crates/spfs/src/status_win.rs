// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::storage::fs::RenderSummary;
use crate::{runtime, Result};

/// Remount the given runtime as configured.
pub async fn remount_runtime(_rt: &runtime::Runtime) -> Result<()> {
    todo!()
}

/// Exit the given runtime as configured, this should only ever be called with the active runtime
pub async fn exit_runtime(_rt: &runtime::Runtime) -> Result<()> {
    todo!()
}

/// Turn the given runtime into a durable runtime, this should only
/// ever be called with the active runtime
pub async fn make_runtime_durable(rt: &runtime::Runtime) -> Result<()> {
    todo!()
}
/// Reinitialize the current spfs runtime as a durable rt (after runtime config changes).
///
/// This function will run blocking IO on the current thread. Although this is not ideal,
/// the mount namespacing operated per-thread and so restricts our ability to move execution.
pub async fn change_to_durable_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    todo!()
}

/// Reinitialize the current spfs runtime as rt (in case of runtime config changes).
///
/// This function will run blocking IO on the current thread. Although this is not ideal,
/// the mount namespacing operated per-thread and so restricts our ability to move execution.
pub async fn reinitialize_runtime(_rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    todo!()
}

/// Initialize the current runtime as rt.
///
/// This function will run blocking IO on the current thread. Although this is not ideal,
/// the mount namespacing operated per-thread and so restricts our ability to move execution.
pub async fn initialize_runtime(_rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    todo!()
}
