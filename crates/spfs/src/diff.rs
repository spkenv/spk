// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;
use std::sync::Arc;

use super::resolve::compute_manifest;
use super::runtime;
use super::status::{active_runtime, compute_runtime_manifest};
use crate::{Error, Result, tracking};

///  Return the changes going from 'base' to 'top'.
///
/// Args:
/// - **base**: The tag or id to use as the base of the computed diff
///   (defaults to the current runtime)
/// - **top**: The tag or id to diff the base against
///   (defaults to the contents of /spfs)
pub async fn diff(
    base: Option<&String>,
    top: Option<&String>,
) -> Result<Vec<tracking::Diff<(), ()>>> {
    let base_manifest = match base {
        None => {
            tracing::debug!("computing runtime manifest as base");
            let runtime = active_runtime().await?;
            compute_runtime_manifest(&runtime).await?
        }
        Some(base) => {
            tracing::debug!(reference = %base, "computing base manifest");
            compute_manifest(base).await?
        }
    };

    let top_manifest = match top {
        None => {
            tracing::debug!("computing manifest for /spfs");
            tracking::compute_manifest("/spfs").await?
        }
        Some(top) => {
            tracing::debug!(reference = ?top, "computing top manifest");
            compute_manifest(top).await?
        }
    };

    tracing::debug!("computing diffs");
    Ok(tracking::compute_diff(&base_manifest, &top_manifest))
}

/// Build a manifest of the current set of changes
/// made to the active runtime
pub async fn runtime_active_changes() -> Result<tracking::Manifest> {
    let config = crate::get_config()?;
    let repo = Arc::new(config.get_local_repository_handle().await?);
    let runtime = active_runtime().await?;
    let changes_dir = get_runtime_changes_dir(&runtime)?;
    crate::Committer::new(&repo)
        .manifest_for_path(&changes_dir)
        .await
        .map(|(_, manifest)| manifest)
}

/// Get the directory containing uncommitted changes for a runtime.
///
/// This is the upper_dir for overlayfs-based backends, or the scratch
/// directory for FuseWithScratch on macOS.
fn get_runtime_changes_dir(runtime: &runtime::Runtime) -> Result<PathBuf> {
    match runtime.config.mount_backend {
        runtime::MountBackend::OverlayFsWithRenders | runtime::MountBackend::OverlayFsWithFuse => {
            // Linux: read from overlayfs upper directory
            Ok(runtime.config.upper_dir.clone())
        }
        runtime::MountBackend::FuseWithScratch => {
            // macOS: read from scratch directory (in ~/Library/Caches/spfs/scratch/)
            let scratch_dir = dirs::cache_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("spfs")
                .join("scratch")
                .join(runtime.name());
            if !scratch_dir.exists() {
                // Create it if needed (the FUSE mount may not have created it yet)
                std::fs::create_dir_all(&scratch_dir).map_err(|e| {
                    Error::String(format!(
                        "Failed to create scratch directory {}: {}",
                        scratch_dir.display(),
                        e
                    ))
                })?;
            }
            Ok(scratch_dir)
        }
        runtime::MountBackend::FuseOnly | runtime::MountBackend::WinFsp => {
            Err(Error::String(format!(
                "Backend {} does not support tracking changes (read-only)",
                runtime.config.mount_backend
            )))
        }
    }
}

/// Return the changes found in the current runtime.
///
/// Unlike [`diff`] this returns only the modifications found in the active
/// runtime. It will not return any [`tracking::DiffMode::Unchanged`] results.
pub async fn diff_runtime_changes() -> Result<Vec<tracking::Diff<(), ()>>> {
    let runtime_manifest = tokio::spawn(async {
        let runtime = active_runtime().await?;
        compute_runtime_manifest(&runtime).await
    });

    let upperdir_manifest = tokio::spawn(runtime_active_changes());

    tracing::debug!("computing diffs");
    let mut raw_diff =
        tracking::compute_diff(&runtime_manifest.await??, &upperdir_manifest.await??);

    // Filter out `DiffMode::Removed` entries that aren't `EntryKind::Mask`.
    // Since we didn't provide the complete manifest for all of /spfs, but
    // just for the overlayfs upperdir instead, anything that wasn't changed
    // will show up in the diff as removed.
    //
    // Also filter out `DiffMode::Unchanged` as we promise not to return
    // any of those either.
    raw_diff.retain(|d| match &d.mode {
        tracking::DiffMode::Removed(e) if e.kind == tracking::EntryKind::Mask => true,
        tracking::DiffMode::Removed(_) => false,
        tracking::DiffMode::Unchanged(_) => false,
        _ => true,
    });

    Ok(raw_diff)
}
