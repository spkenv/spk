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

    match storage::remote_repository("origin")
        .await?
        .read_spec(&pkg)
        .await
    {
        Err(Error::PackageNotFoundError(_)) => {
            storage::local_repository().await?.read_spec(&pkg).await
        }
        res => res,
    }
}

/// Save a package spec to the local repository.
pub async fn save_spec(spec: &api::Spec) -> Result<()> {
    let repo = storage::local_repository().await?;
    repo.force_publish_spec(spec).await
}
