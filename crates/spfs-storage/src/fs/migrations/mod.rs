// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::{Path, PathBuf};

use super::read_last_migration_version;
use crate::{Error, Result};

type MigrationFn = dyn (Fn(&PathBuf, &PathBuf) -> Result<()>) + Sync;

static MIGRATIONS: Vec<(&str, &MigrationFn)> = vec![];

/// Migrate a repository to the latest version and replace the existing data.
pub async fn upgrade_repo<P: AsRef<Path>>(root: P) -> Result<PathBuf> {
    let root = root
        .as_ref()
        .canonicalize()
        .map_err(|err| Error::InvalidPath(root.as_ref().to_owned(), err))?;
    let repo_name = match &root.file_name() {
        None => return Err("Repository path must have a file name".into()),
        Some(name) => name.to_string_lossy(),
    };
    tracing::info!("migrating data...");
    let migrated_path = migrate_repo(&root).await?;
    tracing::info!("swapping out migrated data...");
    let backup_path = root.with_file_name(format!("{repo_name}-backup"));
    tokio::fs::rename(&root, &backup_path)
        .await
        .map_err(|err| Error::StorageWriteError(backup_path.clone(), err))?;
    tokio::fs::rename(&migrated_path, &root)
        .await
        .map_err(|err| Error::StorageWriteError(root.clone(), err))?;
    tracing::info!("purging old data...");
    tokio::fs::remove_dir_all(&backup_path)
        .await
        .map_err(|err| Error::StorageWriteError(backup_path, err))?;
    Ok(root)
}

/// Migrate a repository at the given path to the latest version.
///
/// # Returns:
///    - the path to the migrated repo data
pub async fn migrate_repo<P: AsRef<Path>>(root: P) -> Result<PathBuf> {
    let mut root = root
        .as_ref()
        .canonicalize()
        .map_err(|err| Error::InvalidPath(root.as_ref().to_owned(), err))?;
    let last_migration = read_last_migration_version(&root)
        .await?
        .unwrap_or_else(|| {
            semver::Version::parse(crate::VERSION).expect("crate::VERSION is a valid semver value")
        });
    let repo_name = match &root.file_name() {
        None => return Err("Repository path must have a file name".into()),
        Some(name) => name.to_string_lossy().to_string(),
    };

    for (version, migration_func) in MIGRATIONS.iter() {
        let version = semver::Version::parse(version).unwrap();
        if last_migration.major >= version.major {
            tracing::info!(
                "skip unnecessary migration [{:?} >= {:?}]",
                last_migration,
                version
            );
            continue;
        }

        let migrated_path = root.with_file_name(format!("{repo_name}-{version}"));
        if migrated_path.exists() {
            return Err(format!("found existing migration data: {:?}", migrated_path).into());
        }
        tracing::info!("migrating data from {last_migration} to {version}...");
        migration_func(&root, &migrated_path)?;
        root = root.with_file_name(format!("{repo_name}-migrated"));
        tokio::fs::rename(&migrated_path, &root)
            .await
            .map_err(|err| Error::StorageWriteError(root.clone(), err))?;
    }

    Ok(root)
}
