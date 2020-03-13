import argparse
from datetime import datetime, timedelta

import spfs

from colorama import Fore, Style


def register(sub_parsers: argparse._SubParsersAction) -> None:

    clean_cmd = sub_parsers.add_parser("clean", help=_clean.__doc__)
    clean_cmd.add_argument(
        "--remote", "-r", help=("Trigger the clean operation on a remote repository"),
    )
    clean_cmd.add_argument(
        "--prune",
        default=False,
        action="store_true",
        help="Also prune old tag history in order to clean more data",
    )
    clean_cmd.add_argument(
        "--dry-run",
        "--dry",
        action="store_true",
        help="don't remove anything, just print what would be removed",
    )
    clean_cmd.set_defaults(func=_clean)


def _clean(args: argparse.Namespace) -> None:
    """Clean the repository storage of untracked data."""

    config = spfs.get_config()
    if args.remote is not None:
        repo = config.get_remote(args.remote)
    else:
        repo = config.get_repository()

    if args.prune:

        # last ten items and any items newer than 1 week are kept no matter what
        # other versions older than 9 weeks or greater than 50 are deleted
        params = spfs.PruneParameters(
            prune_if_older_than=datetime.now() - timedelta(weeks=9),
            keep_if_newer_than=datetime.now() - timedelta(weeks=1),
            prune_if_version_more_than=50,
            keep_if_version_less_than=10,
        )
        if args.dry_run:
            for tag in spfs.get_prunable_tags(repo.tags, params):
                print(tag)
            return
        else:
            spfs.prune_tags(repo.tags, params)

    if args.dry_run:
        unattached = spfs.get_all_unattached_objects(repo)
        for digest in unattached:
            print(spfs.io.format_digest(digest))
    else:
        spfs.clean_untagged_objects(repo)
