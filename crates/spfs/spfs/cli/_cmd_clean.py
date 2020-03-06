import argparse

import spfs

from colorama import Fore, Style


def register(sub_parsers: argparse._SubParsersAction) -> None:

    clean_cmd = sub_parsers.add_parser("clean", help=_clean.__doc__)
    clean_cmd.add_argument(
        "--remote",
        "-r",
        help=("Show trigger the clean operation on a remote repository"),
    )
    clean_cmd.set_defaults(func=_clean)


def _clean(args: argparse.Namespace) -> None:
    """Clean the repository storage of untracked data."""

    config = spfs.get_config()
    if args.remote is not None:
        repo = config.get_remote(args.remote)
    else:
        repo = config.get_repository()

    assert isinstance(
        repo, spfs.storage.fs.Repository
    ), f"Can only clean filesystem repositories, got: {repo.__class__.__name}"

    spfs.clean_untagged_objects(repo)
