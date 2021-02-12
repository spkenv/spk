import argparse

import spfs


def register(sub_parsers: argparse._SubParsersAction) -> None:

    info_cmd = sub_parsers.add_parser("version", help=_version.__doc__)
    info_cmd.set_defaults(func=_version)


def _version(args: argparse.Namespace) -> None:
    """Print the spfs version number and exit."""

    print(spfs.__version__)
