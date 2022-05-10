// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::config::get_config;
use super::status::remount_runtime;
use crate::prelude::*;
use crate::{graph, runtime, Error, Result};

#[cfg(test)]
#[path = "./commit_test.rs"]
mod commit_test;

/// Commit the working file changes of a runtime to a new layer.
pub async fn commit_layer(runtime: &mut runtime::Runtime) -> Result<graph::Layer> {
    let config = get_config()?;
    let repo = config.get_repository().await?;
    let manifest = repo
        .commit_dir(runtime.config().upper_dir.as_path())
        .await?;
    if manifest.is_empty() {
        return Err(Error::NothingToCommit);
    }
    let layer = repo.create_layer(&graph::Manifest::from(&manifest)).await?;
    runtime.push_digest(&layer.digest()?)?;
    runtime.set_editable(false)?;
    remount_runtime(runtime).await?;
    Ok(layer)
}

/// Commit the full layer stack and working files to a new platform.
pub async fn commit_platform(runtime: &mut runtime::Runtime) -> Result<graph::Platform> {
    let config = get_config()?;
    let repo = config.get_repository().await?;

    match commit_layer(runtime).await {
        Ok(_) | Err(Error::NothingToCommit) => (),
        Err(err) => return Err(err),
    }

    let stack = runtime.get_stack();
    if stack.is_empty() {
        Err(Error::NothingToCommit)
    } else {
        repo.create_platform(stack.clone()).await
    }
}
