// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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
