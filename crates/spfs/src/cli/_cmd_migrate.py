import os
import argparse

import structlog

import spfs
import spfs.storage.fs.migrations

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    migrate_cmd = sub_parsers.add_parser("migrate", help=_migrate.__doc__)
    migrate_cmd.add_argument(
        "path",
        metavar="PATH",
        help="the path to the filesystem repository to migrate",
    )
    migrate_cmd.add_argument(
        "--upgrade",
        default=False,
        action="store_true",
        help="replace the old data with the migrated data once complete",
    )
    migrate_cmd.set_defaults(func=_migrate)


def _migrate(args: argparse.Namespace) -> None:
    """Migrate the data from and older repository format to the latest one."""

    repo_root = os.path.abspath(args.path)
    if args.upgrade:
        spfs.storage.fs.migrations.upgrade_repo(repo_root)
        result = repo_root
    else:
        result = spfs.storage.fs.migrations.migrate_repo(repo_root)
    _logger.info("migrated", path=result)
