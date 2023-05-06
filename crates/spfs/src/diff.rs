// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use super::resolve::compute_manifest;
use super::status::{active_runtime, compute_runtime_manifest};
use crate::{tracking, Result};

///  Return the changes going from 'base' to 'top'.
///
/// Args:
/// - **base**: The tag or id to use as the base of the computed diff
///         (defaults to the current runtime)
/// - **top**: The tag or id to diff the base against
///         (defaults to the contents of /spfs)
pub async fn diff(base: Option<&String>, top: Option<&String>) -> Result<Vec<tracking::Diff>> {
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

/// Return the changes found in the current runtime.
///
/// Unlike [`diff`] this returns only the modifications found in the active
/// runtime. It will not return any [`tracking::DiffMode::Unchanged`] results.
pub async fn diff_runtime_changes() -> Result<Vec<tracking::Diff>> {
    let runtime_manifest = tokio::spawn(async {
        let runtime = active_runtime().await?;
        compute_runtime_manifest(&runtime).await
    });

    let upperdir_manifest = tokio::spawn(async {
        let config = crate::get_config()?;
        let repo = Arc::new(config.get_local_repository_handle().await?);
        let mut runtime = active_runtime().await?;
        let layer = crate::Committer::new(&repo)
            .commit_layer(&mut runtime)
            .await?;
        Ok::<_, crate::Error>(
            repo.read_manifest(layer.manifest)
                .await?
                .to_tracking_manifest(),
        )
    });

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
