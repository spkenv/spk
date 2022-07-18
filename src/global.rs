// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{convert::TryInto, sync::Arc};

use crate::{
    api,
    storage::{self, Repository},
    Error, Result,
};

#[cfg(test)]
#[path = "./global_test.rs"]
mod global_test;

/// Load a package spec from the default repository.
pub async fn load_spec<S: TryInto<api::Ident, Error = crate::Error>>(
    pkg: S,
) -> Result<Arc<api::Spec>> {
    let pkg = pkg.try_into()?;

    // Do not require "origin" to exist.
    match storage::remote_repository("origin").await {
        Ok(repo) => match repo.read_spec(&pkg).await {
            Ok(spec) => return Ok(spec),
            Err(Error::PackageNotFoundError(_)) => {}
            Err(err) => return Err(err),
        },
        Err(Error::SPFS(spfs::Error::FailedToOpenRepository { source, .. }))
            if matches!(*source, spfs::Error::UnknownRemoteName(_)) => {}
        Err(err) => return Err(err),
    }

    storage::local_repository().await?.read_spec(&pkg).await
}

/// Save a package spec to the local repository.
pub async fn save_spec(spec: &api::Spec) -> Result<()> {
    let repo = storage::local_repository().await?;
    repo.force_publish_spec(spec).await
}
