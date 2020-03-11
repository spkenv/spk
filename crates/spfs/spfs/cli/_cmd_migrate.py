import os
import argparse

import structlog

import spfs
import spfs.storage.fs.migrations

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    migrate_cmd = sub_parsers.add_parser("migrate", help=_migrate.__doc__)
    migrate_cmd.add_argument(
        "path", metavar="PATH", help="the path to the filesystem repository to migrate",
    )
    migrate_cmd.set_defaults(func=_migrate)


def _migrate(args: argparse.Namespace) -> None:
    """Migrate the data in and older repository format."""

    repo_root = os.path.abspath(args.path)
    spfs.storage.fs.migrations.migrate_repo(repo_root)
