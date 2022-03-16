// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::Path;

use futures::TryStreamExt;

use super::{Repository, SPFSRepository};
use crate::{api, Error, Result};

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

    let local_repo = crate::HANDLE.block_on(super::local_repository())?;
    let remote_repo = crate::HANDLE.block_on(super::remote_repository("origin"))?;
    let mut target_repo = super::SPFSRepository::from(spfs::storage::RepositoryHandle::from(
        crate::HANDLE.block_on(spfs::storage::tar::TarRepository::create(&filename))?,
    ));

    let mut to_transfer = std::collections::HashSet::new();
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
        // but the remote was always a fallback and fixing the
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

pub async fn import_package<P: AsRef<Path>>(filename: P) -> Result<()> {
    let tar_repo: spfs::storage::RepositoryHandle =
        spfs::storage::tar::TarRepository::open(filename)
            .await?
            .into();
    let local_repo = super::local_repository().await?;

    let mut stream = tar_repo.iter_tags();
    while let Some((tag, _)) = stream.try_next().await? {
        tracing::info!(?tag, "importing");
        spfs::sync_ref(tag.to_string(), &local_repo, &tar_repo).await?;
    }
    Ok(())
}

fn copy_package(
    pkg: &api::Ident,
    src_repo: &SPFSRepository,
    dst_repo: &mut SPFSRepository,
) -> Result<()> {
    let spec = src_repo.read_spec(pkg)?;
    if pkg.build.is_none() {
        tracing::info!(?pkg, "exporting");
        dst_repo.publish_spec(spec)?;
        return Ok(());
    }

    let components = src_repo.get_package(pkg)?;
    tracing::info!(?pkg, "exporting");
    for (_name, digest) in components.iter() {
        crate::HANDLE.block_on(spfs::sync_ref(digest.to_string(), src_repo, dst_repo))?;
    }
    dst_repo.publish_package(spec, components)?;
    Ok(())
}
