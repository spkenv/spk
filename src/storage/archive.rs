// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::Path;

use futures::TryStreamExt;

use super::{Repository, SPFSRepository};
use crate::{api, Error, Result};

#[cfg(test)]
#[path = "./archive_test.rs"]
mod archive_test;

pub fn export_package<P: AsRef<Path>>(pkg: &api::Ident, filename: P) -> Result<()> {
    // Make filename absolute as spfs::runtime::makedirs_with_perms does not handle
    // relative paths properly.
    let filename = std::env::current_dir()?.join(filename);

    if let Err(err) = std::fs::remove_file(&filename) {
        match err.kind() {
            std::io::ErrorKind::NotFound => (),
            _ => tracing::warn!("Error trying to remove old file: {:?}", err),
        }
    }

    filename
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or_else(|| Ok(()))?;

    let (local_repo, remote_repo) = crate::HANDLE.block_on(async {
        tokio::try_join!(
            super::local_repository(),
            super::remote_repository("origin"),
        )
    })?;
    let mut target_repo = super::SPFSRepository::from(spfs::storage::RepositoryHandle::from(
        crate::HANDLE.block_on(spfs::storage::tar::TarRepository::create(&filename))?,
    ));

    // these are sorted to ensure that the version spec is published
    // before any build - it's only an error in testing, but still best practice
    let mut to_transfer = std::collections::BTreeSet::new();
    to_transfer.insert(pkg.clone());
    if pkg.build.is_none() {
        to_transfer.extend(local_repo.list_package_builds(pkg)?);
        to_transfer.extend(remote_repo.list_package_builds(pkg)?);
    } else {
        to_transfer.insert(pkg.with_build(None));
    }

    for pkg in to_transfer.into_iter() {
        let local_err = match copy_package(&pkg, &local_repo, &mut target_repo) {
            Ok(_) => continue,
            Err(Error::PackageNotFoundError(_)) => None,
            Err(err) => Some(err),
        };
        let remote_err = match copy_package(&pkg, &remote_repo, &mut target_repo) {
            Ok(_) => continue,
            Err(Error::PackageNotFoundError(_)) => None,
            Err(err) => Some(err),
        };
        // we will hide the remote_err in cases when both failed,
        // because the remote was always a fallback and fixing the
        // local error is preferred
        return Err(local_err
            .or(remote_err)
            .unwrap_or(Error::PackageNotFoundError(pkg)));
    }

    tracing::info!(path=?filename, "building archive");
    use std::ops::DerefMut;
    if let spfs::storage::RepositoryHandle::Tar(tar) = target_repo.deref_mut() {
        tar.flush()?;
    }
    Ok(())
}

pub async fn import_package<P: AsRef<Path>>(filename: P) -> Result<spfs::sync::SyncEnvResult> {
    let tar_repo: spfs::storage::RepositoryHandle =
        spfs::storage::tar::TarRepository::open(filename.as_ref())
            .await?
            .into();
    let local_repo = super::local_repository().await?;

    let env_spec = tar_repo
        .iter_tags()
        .map_ok(|(spec, _)| spec)
        .try_collect()
        .await?;
    tracing::info!(archive = ?filename.as_ref(), "importing");
    let result = spfs::Syncer::new(&tar_repo, &local_repo)
        .with_reporter(spfs::sync::ConsoleSyncReporter::default())
        .sync_env(env_spec)
        .await?;
    Ok(result)
}

fn copy_package(
    pkg: &api::Ident,
    src_repo: &SPFSRepository,
    dst_repo: &mut SPFSRepository,
) -> Result<()> {
    let spec = src_repo.read_spec(pkg)?;
    if pkg.build.is_none() {
        tracing::info!(%pkg, "exporting version spec");
        dst_repo.publish_spec(&spec)?;
        return Ok(());
    }

    let components = src_repo.get_package(pkg)?;
    let env_spec = components.values().cloned().collect();
    tracing::info!(%pkg, "exporting build");
    let syncer = spfs::Syncer::new(src_repo, dst_repo)
        .with_reporter(spfs::sync::ConsoleSyncReporter::default());
    let future = syncer.sync_env(env_spec);
    let _result = crate::HANDLE.block_on(future)?;
    dst_repo.publish_package(&spec, components)?;
    Ok(())
}
