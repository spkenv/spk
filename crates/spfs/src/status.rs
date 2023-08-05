// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#[cfg_attr(unix, path = "./status_unix.rs")]
#[cfg_attr(windows, path = "./status_win.rs")]
mod os;

pub use os::*;

use super::config::get_config;
use crate::storage::FromConfig;
use crate::{runtime, tracking, Error, Result};

static SPFS_RUNTIME: &str = "SPFS_RUNTIME";

/// Unlock the current runtime file system so that it can be modified.
///
/// Once modified, active changes can be committed
///
/// Errors:
/// - [`Error::NoActiveRuntime`]: if there is no active runtime
/// - [`Error::RuntimeAlreadyEditable`]: if the active runtime is already editable
/// - if there are issues remounting the filesystem
pub async fn make_active_runtime_editable() -> Result<()> {
    let mut rt = active_runtime().await?;
    if rt.status.editable {
        return Err(Error::RuntimeAlreadyEditable);
    }

    rt.status.editable = true;
    rt.save_state_to_storage().await?;
    remount_runtime(&rt).await
}

/// Get the repository that is being used as the backing storage for the given
/// runtime.
///
/// This is usually the local repository but it may be a remote repository
/// depending on the configuration of the spfs filesystem backend.
pub async fn get_runtime_backing_repo(
    rt: &runtime::Runtime,
) -> Result<crate::storage::RepositoryHandle> {
    let config = get_config()?;
    if rt.config.mount_backend.requires_localization() {
        config.get_local_repository_handle().await
    } else {
        let proxy_config = crate::storage::proxy::Config {
            primary: config.storage.root.to_string_lossy().to_string(),
            secondary: rt
                .config
                .secondary_repositories
                .iter()
                .map(ToString::to_string)
                .collect(),
        };
        Ok(crate::storage::ProxyRepository::from_config(proxy_config)
            .await?
            .into())
    }
}

/// Calculate the file manifest for the layers in the given runtime.
///
/// The returned manifest DOES NOT include any active changes to the runtime.
pub async fn compute_runtime_manifest(rt: &runtime::Runtime) -> Result<tracking::Manifest> {
    let spec = rt.status.stack.iter().cloned().collect();
    super::compute_environment_manifest(&spec, &get_runtime_backing_repo(rt).await?).await
}

/// Return the currently active runtime
///
/// # Errors:
/// - [`Error::NoActiveRuntime`] if there is no runtime detected
/// - [`Error::UnknownRuntime`] if the environment references a
///   runtime that is not in the configured runtime storage
/// - other issues loading the config or accessing the runtime data
pub async fn active_runtime() -> Result<runtime::Runtime> {
    let name = std::env::var(SPFS_RUNTIME).map_err(|_| Error::NoActiveRuntime)?;
    let config = get_config()?;
    let storage = config.get_runtime_storage().await?;
    storage.read_runtime(name).await
}
