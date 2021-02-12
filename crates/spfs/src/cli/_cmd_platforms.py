import argparse

import spfs


def register(sub_parsers: argparse._SubParsersAction) -> None:

    platforms_cmd = sub_parsers.add_parser("platforms", help=_platforms.__doc__)
    platforms_cmd.add_argument(
        "--remote",
        "-r",
        help="Show platforms from remote repository instead of the local one",
    )
    platforms_cmd.set_defaults(func=_platforms)


def _platforms(args: argparse.Namespace) -> None:
    """List all platforms in an spfs repository."""

    config = spfs.get_config()
    if args.remote is not None:
        repo = config.get_remote(args.remote)
    else:
        repo = config.get_repository()

    platforms = repo.iter_platforms()
    for platform in platforms:
        print(spfs.io.format_digest(platform.digest()))
