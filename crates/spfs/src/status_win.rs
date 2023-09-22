// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::storage::fs::RenderSummary;
use crate::{env, runtime, Error, Result};

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
pub async fn change_to_durable_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    Err(Error::OverlayFsUnsupportedOnWindows)
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
pub async fn initialize_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    tracing::debug!("computing runtime manifest");
    let manifest = super::compute_runtime_manifest(rt).await?;

    let configurator = env::RuntimeConfigurator::default();
    match rt.config.mount_backend {
        #[cfg(feature = "winfsp-backend")]
        runtime::MountBackend::WinFsp => {
            configurator.mount_env_winfsp(rt).await?;
        }
        #[allow(unreachable_patterns)]
        _ => {
            return Err(Error::String(format!(
                "This binary was not compiled with support for {}",
                rt.config.mount_backend
            )))
        }
    }
    Ok(RenderSummary::default())
}
