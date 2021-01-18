import argparse

import spfs


def register(sub_parsers: argparse._SubParsersAction) -> None:

    tags_cmd = sub_parsers.add_parser("tags", help=_tags.__doc__)
    tags_cmd.add_argument(
        "--remote",
        "-r",
        help="Show tags from remote repository instead of the local one",
    )
    tags_cmd.set_defaults(func=_tags)


def _tags(args: argparse.Namespace) -> None:
    """List all tags in an spfs repository."""

    config = spfs.get_config()
    if args.remote is not None:
        repo = config.get_remote(args.remote)
    else:
        repo = config.get_repository()

    for _, tag in repo.iter_tags():
        print(spfs.io.format_digest(tag.target))
