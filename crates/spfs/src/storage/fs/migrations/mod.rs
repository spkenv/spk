// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;

use std::path::{Path, PathBuf};

pub use error::{MigrationError, MigrationResult};

use super::read_last_migration_version;

type MigrationFn = dyn (Fn(&PathBuf, &PathBuf) -> MigrationResult<()>) + Sync;

static MIGRATIONS: Vec<(&str, &MigrationFn)> = vec![];

/// Migrate a repository to the latest version and replace the existing data.
pub async fn upgrade_repo<P: AsRef<Path>>(root: P) -> MigrationResult<PathBuf> {
    let root = tokio::task::block_in_place(|| dunce::canonicalize(&root))
        .map_err(|err| MigrationError::InvalidRoot(root.as_ref().to_owned(), err))?;
    let repo_name = match &root.file_name() {
        None => return Err(MigrationError::NoFileName),
        Some(name) => name.to_string_lossy(),
    };
    tracing::info!("migrating data...");
    let migrated_path = migrate_repo(&root).await?;
    tracing::info!("swapping out migrated data...");
    let backup_path = root.with_file_name(format!("{repo_name}-backup"));
    tokio::fs::rename(&root, &backup_path)
        .await
        .map_err(|err| {
            MigrationError::WriteError("rename on repo to backup", backup_path.clone(), err)
        })?;
    tokio::fs::rename(&migrated_path, &root)
        .await
        .map_err(|err| {
            MigrationError::WriteError("rename on migrated path to root", root.clone(), err)
        })?;
    tracing::info!("purging old data...");
    tokio::fs::remove_dir_all(&backup_path)
        .await
        .map_err(|err| MigrationError::WriteError("remove_all_dir on backup", backup_path, err))?;
    Ok(root)
}

/// Migrate a repository at the given path to the latest version.
///
/// # Returns:
///    - the path to the migrated repo data
pub async fn migrate_repo<P: AsRef<Path>>(root: P) -> MigrationResult<PathBuf> {
    let mut root = tokio::task::block_in_place(|| dunce::canonicalize(&root))
        .map_err(|err| MigrationError::InvalidRoot(root.as_ref().to_owned(), err))?;
    let last_migration = read_last_migration_version(&root)
        .await?
        .unwrap_or_else(|| {
            semver::Version::parse(crate::VERSION).expect("crate::VERSION is a valid semver value")
        });
    let repo_name = match &root.file_name() {
        None => return Err(MigrationError::NoFileName),
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
            return Err(MigrationError::ExistingData(migrated_path));
        }
        tracing::info!("migrating data from {last_migration} to {version}...");
        migration_func(&root, &migrated_path)?;
        root = root.with_file_name(format!("{repo_name}-migrated"));
        tokio::fs::rename(&migrated_path, &root)
            .await
            .map_err(|err| {
                MigrationError::WriteError("rename on migrated repo", root.clone(), err)
            })?;
    }

    Ok(root)
}
