import argparse

from colorama import Fore

import spfs


def register(sub_parsers: argparse._SubParsersAction) -> None:

    config = spfs.get_config()

    pull_cmd = sub_parsers.add_parser("pull", help=_pull.__doc__)
    pull_cmd.add_argument(
        "refs", metavar="REF", nargs="+", help="the references to pull"
    )
    pull_cmd.add_argument(
        "--remote",
        "-r",
        help=(
            "the name or address of the remote server to pull from, "
            "defaults to searching all configured remotes"
        ),
    )
    pull_cmd.set_defaults(func=_pull)


def _pull(args: argparse.Namespace) -> None:
    """Pull one or more objects to the local reposotory."""

    if args.remote is None:
        for ref in args.refs:
            spfs.pull_ref(ref)
        return

    config = spfs.get_config()
    repo = config.get_repository()
    remote = config.get_remote(args.remote)
    for ref in args.refs:
        spfs.sync_ref(ref, remote, repo)
