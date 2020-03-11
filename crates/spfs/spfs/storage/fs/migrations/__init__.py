from typing import Dict, Callable
import os
import shutil

import semver
import structlog

from .... import __version__
from .. import read_last_migration_version
from ._016 import migrate

_LOGGER = structlog.get_logger("spfs.storage.fs.migrations")
MIGRATIONS: Dict[str, Callable[[str, str], None]] = {
    "0.16.0": migrate,
}


def migrate_repo(root: str) -> None:
    """Migrate a repository at the given path to the latest version."""

    repo_root = os.path.abspath(root)
    last_migration = read_last_migration_version(repo_root)

    for version, migration_func in MIGRATIONS.items():
        if semver.compare(last_migration, version) >= 0:
            _LOGGER.info(
                f"skip migration for older version: {last_migration} > {version}"
            )
            continue

        backup_path = repo_root.rstrip("/") + f"-backup"
        migrated_path = repo_root.rstrip("/") + f"-{version}"
        _LOGGER.info(f"migrating data from {last_migration} to {version}...")
        migration_func(repo_root, migrated_path)
        _LOGGER.info(f"swapping out migrated data...")
        os.rename(repo_root, backup_path)
        os.rename(migrated_path, repo_root)
        _LOGGER.info("purging old data...")
        shutil.rmtree(backup_path)
        last_migration = version
