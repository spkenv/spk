import argparse

import spenv


def register(sub_parsers: argparse._SubParsersAction) -> None:

    show_cmd = sub_parsers.add_parser("show", help=_show.__doc__)
    show_cmd.add_argument("refs", metavar="REF", nargs="+")
    show_cmd.set_defaults(func=_show)


def _show(args: argparse.Namespace) -> None:
    """Display information about one or more items."""

    config = spenv.get_config()
    repo = config.get_repository()
    for ref in args.refs:
        layer = repo.read_ref(ref)
        print(repr(layer))
