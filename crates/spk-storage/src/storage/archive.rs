// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryFrom;
use std::path::Path;

use spk_schema::{AnyIdent, BuildIdent, VersionIdent};

use super::{Repository, SPFSRepository};
use crate::{Error, Result};

pub async fn export_package<I, P>(pkg: I, filename: P) -> Result<()>
where
    I: AsRef<AnyIdent>,
    P: AsRef<Path>,
{
    let pkg = pkg.as_ref();
    // Make filename absolute as spfs::runtime::makedirs_with_perms does not handle
    // relative paths properly.
    let filename = std::env::current_dir()
        .map_err(|err| Error::String(format!("Failed to get current directory: {err}")))?
        .join(filename);

    if let Err(err) = std::fs::remove_file(&filename) {
        match err.kind() {
            std::io::ErrorKind::NotFound => (),
            _ => tracing::warn!("Error trying to remove old file: {:?}", err),
        }
    }

    filename
        .parent()
        .map(|dir| {
            std::fs::create_dir_all(dir)
                .map_err(|err| Error::DirectoryCreateError(dir.to_owned(), err))
        })
        .unwrap_or_else(|| Ok(()))?;

    // Don't require the "origin" repo to exist here.
    let (local_repo, remote_repo) = tokio::join!(
        super::local_repository(),
        super::remote_repository("origin"),
    );
    let local_repo = local_repo?;
    let mut target_repo = super::SPFSRepository::try_from((
        "archive",
        spfs::storage::RepositoryHandle::from(
            spfs::storage::tar::TarRepository::create(&filename).await?,
        ),
    ))?;

    // these are sorted to ensure that the recipe is published
    // before any build - it's only an error in testing, but still best practice
    let mut to_transfer = std::collections::BTreeSet::new();
    to_transfer.insert(pkg.clone());
    if pkg.build().is_none() {
        to_transfer.extend(
            local_repo
                .list_package_builds(pkg)
                .await?
                .into_iter()
                .map(|pkg| pkg.into_any()),
        );
        if remote_repo.is_err() {
            return remote_repo.map(|_| ());
        }
        to_transfer.extend(
            remote_repo
                .as_ref()
                .unwrap()
                .list_package_builds(pkg)
                .await?
                .into_iter()
                .map(|pkg| pkg.into_any()),
        );
    } else {
        to_transfer.insert(pkg.with_build(None));
    }

    for pkg in to_transfer.into_iter() {
        let local_err = match copy_any(pkg.clone(), &local_repo, &target_repo).await {
            Ok(_) => continue,
            Err(Error::SpkValidatorsError(
                spk_schema::validators::Error::PackageNotFoundError(_),
            )) => None,
            Err(err) => Some(err),
        };
        if remote_repo.is_err() {
            return remote_repo.map(|_| ());
        }
        let remote_err =
            match copy_any(pkg.clone(), remote_repo.as_ref().unwrap(), &target_repo).await {
                Ok(_) => continue,
                Err(Error::SpkValidatorsError(
                    spk_schema::validators::Error::PackageNotFoundError(_),
                )) => None,
                Err(err) => Some(err),
            };
        // we will hide the remote_err in cases when both failed,
        // because the remote was always a fallback and fixing the
        // local error is preferred
        return Err(local_err
            .or(remote_err)
            .unwrap_or_else(|| spk_schema::validators::Error::PackageNotFoundError(pkg).into()));
    }

    tracing::info!(path=?filename, "building archive");
    use std::ops::DerefMut;
    if let spfs::storage::RepositoryHandle::Tar(tar) = target_repo.deref_mut() {
        tar.flush()?;
    }
    Ok(())
}

async fn copy_any(
    pkg: AnyIdent,
    src_repo: &SPFSRepository,
    dst_repo: &SPFSRepository,
) -> Result<()> {
    match pkg.into_inner() {
        (base, None) => copy_recipe(&base, src_repo, dst_repo).await,
        (base, Some(build)) => {
            copy_package(&BuildIdent::new(base, build), src_repo, dst_repo).await
        }
    }
}

async fn copy_recipe(
    pkg: &VersionIdent,
    src_repo: &SPFSRepository,
    dst_repo: &SPFSRepository,
) -> Result<()> {
    let spec = src_repo.read_recipe(pkg).await?;
    tracing::info!(%pkg, "exporting");
    dst_repo.publish_recipe(&spec).await?;
    Ok(())
}

async fn copy_package(
    pkg: &BuildIdent,
    src_repo: &SPFSRepository,
    dst_repo: &SPFSRepository,
) -> Result<()> {
    let spec = src_repo.read_package(pkg).await?;
    let components = src_repo.read_components(pkg).await?;
    tracing::info!(%pkg, "exporting");
    let syncer = spfs::Syncer::new(src_repo, dst_repo)
        .with_reporter(spfs::sync::ConsoleSyncReporter::default());
    let desired = components.iter().map(|i| *i.1).collect();
    syncer.sync_env(desired).await?;
    dst_repo.publish_package(&spec, &components).await?;
    Ok(())
}
