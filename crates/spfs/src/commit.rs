// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::status::remount_runtime;
use crate::{graph, prelude::*, runtime, Error, Result};

#[cfg(test)]
#[path = "./commit_test.rs"]
mod commit_test;

/// Commit the working file changes of a runtime to a new layer in the provided repo.
pub async fn commit_layer<R>(runtime: &mut runtime::Runtime, repo: &R) -> Result<graph::Layer>
where
    R: Repository + ?Sized,
{
    let manifest = repo.commit_dir(runtime.config.upper_dir.as_path()).await?;
    if manifest.is_empty() {
        return Err(Error::NothingToCommit);
    }
    let layer = repo.create_layer(&graph::Manifest::from(&manifest)).await?;
    runtime.push_digest(&layer.digest()?);
    runtime.status.editable = false;
    runtime.save().await?;
    remount_runtime(runtime).await?;
    Ok(layer)
}

/// Commit the full layer stack and working files to a new platform.
pub async fn commit_platform<R>(runtime: &mut runtime::Runtime, repo: &R) -> Result<graph::Platform>
where
    R: Repository + ?Sized,
{
    match commit_layer(runtime, repo).await {
        Ok(_) | Err(Error::NothingToCommit) => (),
        Err(err) => return Err(err),
    }

    runtime.reload().await?;
    if runtime.status.stack.is_empty() {
        Err(Error::NothingToCommit)
    } else {
        repo.create_platform(runtime.status.stack.clone()).await
    }
}
