from typing import Dict, Callable
import os
import shutil

import semver
import structlog

from .... import __version__, graph
from .. import read_last_migration_version
from ._016 import migrate

_LOGGER = structlog.get_logger("spfs.storage.fs.migrations")
MIGRATIONS: Dict[str, Callable[[str, str], None]] = {
    "0.16.0": migrate,
}


def upgrade_repo(root: str) -> None:
    """Migrate a repository to the latest version and replace the existing data."""

    repo_root = os.path.abspath(root)
    migrated_path = migrate_repo(root)
    _LOGGER.info(f"swapping out migrated data...")
    backup_path = repo_root.rstrip("/") + f"-backup"
    os.rename(repo_root, backup_path)
    os.rename(migrated_path, repo_root)
    _LOGGER.info("purging old data...")
    shutil.rmtree(backup_path)


def migrate_repo(root: str) -> str:
    """Migrate a repository at the given path to the latest version.

    Returns:
        str: the path to the migrated repo data
    """

    repo_root = os.path.abspath(root)
    last_migration = read_last_migration_version(repo_root)

    for version, migration_func in MIGRATIONS.items():
        if semver.compare(last_migration, version) >= 0:
            _LOGGER.info(f"skip unnecessary migration [{last_migration} >= {version}]")
            continue

        migrated_path = repo_root.rstrip("/") + f"-{version}"
        assert not os.path.exists(migrated_path), "found existing migration data"
        _LOGGER.info(f"migrating data from {last_migration} to {version}...")
        migration_func(repo_root, migrated_path)
        repo_root = repo_root.rstrip("/") + f"-migrated"
        os.rename(migrated_path, repo_root)

    return repo_root
