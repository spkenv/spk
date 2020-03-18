import sys
import argparse
from datetime import datetime, timedelta

import structlog
from colorama import Fore, Style

import spfs


_LOGGER = structlog.get_logger("spfs.cli")


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
    prune_args = clean_cmd.add_argument_group("pruning options")
    clean_cmd.add_argument(
        "--yes",
        "--y",
        action="store_true",
        help="Don't prompt/ask before cleaning the data",
    )
    clean_cmd.set_defaults(func=_clean)

    prune_args.add_argument(
        "--prune-if-older-than",
        metavar="AGE",
        default="9w",
        help="Prune tags older that the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s) (default: 9w)",
    )
    prune_args.add_argument(
        "--keep-if-newer-than",
        metavar="AGE",
        default="1w",
        help="Always keep tags newer than the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s) (default: 1w)",
    )
    prune_args.add_argument(
        "--prune-if-more-than",
        metavar="COUNT",
        type=int,
        default=50,
        help="Prune tags if there are more than this number in a stream (default: 50)",
    )
    prune_args.add_argument(
        "--keep-if-less-than",
        metavar="COUNT",
        default=10,
        help="Always keep at least this number of tags in a stream (default: 10)",
    )


def _clean(args: argparse.Namespace) -> None:
    """Clean the repository storage of untracked data."""

    config = spfs.get_config()
    if args.remote is not None:
        repo = config.get_remote(args.remote)
    else:
        repo = config.get_repository()

    if args.prune:
        _prune(args, repo)

    unattached = spfs.get_all_unattached_objects(repo)
    if not len(unattached):
        _LOGGER.info("no objects to remove")
        return
    for digest in unattached:
        print(spfs.io.format_digest(digest))
    if not args.yes:
        answer = input(
            "Do you wish to proceed with the removal of these objects? [y/N]: "
        )
        if answer != "y":
            sys.exit(1)
    for digest in unattached:
        spfs.purge_objects(unattached, repo)


def _prune(args: argparse.Namespace, repo: spfs.storage.Repository) -> None:

    try:
        prune_if_older_than = _age_to_date(args.prune_if_older_than)
        keep_if_newer_than = _age_to_date(args.keep_if_newer_than)
        prune_if_more_than = int(args.prune_if_more_than)
        keep_if_less_than = int(args.keep_if_less_than)
    except ValueError as e:
        print(f"{Fore.RED}{e}{Fore.RESET}")
        sys.exit(1)

    params = spfs.PruneParameters(
        prune_if_older_than=prune_if_older_than,
        keep_if_newer_than=keep_if_newer_than,
        prune_if_version_more_than=prune_if_more_than,
        keep_if_version_less_than=keep_if_less_than,
    )

    _LOGGER.info(f"collecting tags older than {args.prune_if_older_than}")
    _LOGGER.info(f"and collecting tags with version > {args.prune_if_more_than}")
    _LOGGER.info(f"but leaving tags newer than {args.keep_if_newer_than}")
    _LOGGER.info(f"and leaving tags with version <= {args.keep_if_less_than}")

    _LOGGER.info("searching for tags to prune...")
    to_prune = spfs.get_prunable_tags(repo.tags, params)
    if not len(to_prune):
        _LOGGER.info("no tags to prune")
        return

    for tag in to_prune:
        spec = spfs.tracking.build_tag_spec(tag.name, tag.org)
        spec_str = str(spec).ljust(len(tag.path) + 3)
        info = f"{Fore.YELLOW}{tag.target.str()[:10]}{Fore.RESET}"
        info += f" {Style.BRIGHT}{spec_str}{Style.RESET_ALL}"
        info += f" {Fore.LIGHTBLUE_EX}{tag.user}"
        info += f' {Fore.GREEN}{tag.time.strftime("%F %R")}{Fore.RESET}'
        print(info)

    if not args.yes:
        answer = input(
            "Do you wish to proceed with the removal of these tag versions? [y/N]: "
        )
        if answer != "y":
            sys.exit(1)

    for tag in to_prune:
        repo.tags.remove_tag(tag)


def _age_to_date(age: str) -> datetime:

    num, postfix = int(age[:-1]), age[-1]

    postfix_map = {
        "y": "years",
        "w": "weeks",
        "d": "days",
        "h": "hours",
        "m": "minutes",
        "s": "seconds",
    }

    try:
        args = {postfix_map[postfix]: num}
    except KeyError:
        raise ValueError(
            f"Unknown age postfix: '{postfix}', "
            f"must be one of {list(postfix_map.keys())}"
        )
    return datetime.now() - timedelta(**args)
