// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryFrom;
use std::path::Path;

use itertools::{Itertools, Position};
use spk_schema::ident_ops::TagPathStrategy;
use spk_schema::{AnyIdent, BuildIdent, VersionIdent};
use variantly::Variantly;

use super::{Repository, SpfsRepository};
use crate::{Error, NameAndRepositoryWithTagStrategy, Result, SpfsRepositoryHandle};

pub async fn export_package<'a, S>(
    source_repos: &[SpfsRepositoryHandle<'a>],
    pkg: impl AsRef<AnyIdent>,
    filename: impl AsRef<Path>,
) -> Result<()>
where
    S: TagPathStrategy + Send + Sync,
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

    let tar_repo = spfs::storage::tar::TarRepository::create(&filename)
        .await
        .map_err(|source| spfs::Error::FailedToOpenRepository {
            repository: "<TAR Archive>".into(),
            source,
        })?;
    // Package exports should not include the top-level directory for
    // durable runtime upperdir edits.
    tar_repo.remove_durable_dir().await?;

    let target_repo =
        super::SpfsRepository::try_from(NameAndRepositoryWithTagStrategy::<_, _, S>::new(
            "archive",
            spfs::storage::RepositoryHandle::from(tar_repo),
        ))?;

    // these are sorted to ensure that the recipe is published
    // before any build - it's only an error in testing, but still best practice
    let mut to_transfer = std::collections::BTreeSet::new();
    to_transfer.insert(pkg.clone());
    if pkg.build().is_none() {
        for repo in source_repos {
            to_transfer.extend(
                repo.list_package_builds(pkg.as_version())
                    .await?
                    .into_iter()
                    .map(|pkg| pkg.into_any()),
            );
        }
    } else {
        to_transfer.insert(pkg.with_build(None));
    }

    'pkg: for transfer_pkg in to_transfer.into_iter() {
        if transfer_pkg.is_embedded() {
            // Don't attempt to export an embedded package; the stub
            // will be recreated if exporting its provider.
            continue;
        }

        #[derive(Variantly)]
        enum CopyResult {
            VersionNotFound,
            BuildNotFound,
            Err(Error),
        }

        let mut first_error = None;
        let mut all_errors_are_build_not_found = true;

        for (position, repo) in source_repos.iter().with_position() {
            let err = match copy_any(transfer_pkg.clone(), repo, &target_repo).await {
                Ok(_) => continue 'pkg,
                Err(Error::PackageNotFound(ident)) => {
                    if ident.build().is_some() {
                        CopyResult::BuildNotFound
                    } else {
                        CopyResult::VersionNotFound
                    }
                }
                Err(err) => CopyResult::Err(err),
            };

            // `list_package_builds` can return builds that only exist as spfs tags
            // under `spk/spec`, meaning the build doesn't really exist. Ignore
            // `PackageNotFound` about these ... unless the build was
            // explicitly named to be archived.
            //
            // Consider changing `list_package_builds` so it doesn't do that
            // anymore, although it has a comment that it is doing so
            // intentionally. Maybe it should return a richer type that describes
            // if only the "spec build" exists and that info could be used here.
            all_errors_are_build_not_found = all_errors_are_build_not_found
                && matches!(err, CopyResult::BuildNotFound)
                && pkg.build().is_none();

            // We'll report the error from the first repo that failed, under the
            // assumption that the repo(s) listed first are more likely to be
            // where the problem is fixable (e.g., the local repo).
            if first_error.is_none() {
                first_error = Some(err);
            }

            match position {
                Position::Last | Position::Only if all_errors_are_build_not_found => {
                    continue 'pkg;
                }
                Position::Last | Position::Only => {
                    return Err(first_error
                        .unwrap()
                        .err()
                        .unwrap_or_else(|| Error::PackageNotFound(transfer_pkg)));
                }
                _ => {
                    // Try the next repo
                    continue;
                }
            }
        }
    }

    tracing::info!(path=?filename, "building archive");
    use std::ops::Deref;
    if let spfs::storage::RepositoryHandle::Tar(tar) = target_repo.deref() {
        tar.flush()?;
    }
    Ok(())
}

async fn copy_any<'a, S2>(
    pkg: AnyIdent,
    src_repo: &SpfsRepositoryHandle<'a>,
    dst_repo: &SpfsRepository<S2>,
) -> Result<()>
where
    S2: TagPathStrategy + Send + Sync,
{
    match pkg.into_inner() {
        (base, None) => copy_recipe(&base, src_repo, dst_repo).await,
        (base, Some(build)) => {
            copy_package(&BuildIdent::new(base, build), src_repo, dst_repo).await
        }
    }
}

async fn copy_recipe<'a, S2>(
    pkg: &VersionIdent,
    src_repo: &SpfsRepositoryHandle<'a>,
    dst_repo: &SpfsRepository<S2>,
) -> Result<()>
where
    S2: TagPathStrategy + Send + Sync,
{
    let spec = src_repo.read_recipe(pkg).await?;
    tracing::info!(%pkg, "exporting");
    dst_repo.publish_recipe(&spec).await?;
    Ok(())
}

async fn copy_package<'a, S2>(
    pkg: &BuildIdent,
    src_repo: &SpfsRepositoryHandle<'a>,
    dst_repo: &SpfsRepository<S2>,
) -> Result<()>
where
    S2: TagPathStrategy + Send + Sync,
{
    let spec = src_repo.read_package(pkg).await?;
    let components = src_repo.read_components(pkg).await?;
    tracing::info!(%pkg, "exporting");
    let syncer = spfs::Syncer::new(src_repo.spfs_repository_handle(), dst_repo)
        .with_reporter(spfs::sync::ConsoleSyncReporter::default());
    let desired = components.iter().map(|i| *i.1).collect();
    syncer.sync_env(desired).await?;
    dst_repo.publish_package(&spec, &components).await?;
    Ok(())
}
