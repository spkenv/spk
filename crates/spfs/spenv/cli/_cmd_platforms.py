import argparse

import spenv


def register(sub_parsers: argparse._SubParsersAction) -> None:

    platforms_cmd = sub_parsers.add_parser("platforms", help=_platforms.__doc__)
    platforms_cmd.set_defaults(func=_platforms)


def _platforms(args: argparse.Namespace) -> None:

    config = spenv.get_config()
    repo = config.get_repository()
    platforms = repo.platforms.list_platforms()
    for platform in platforms:
        print(spenv.io.format_digest(platform.digest))
