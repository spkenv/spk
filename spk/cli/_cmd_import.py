import sys
from typing import Callable, Any
import os
import argparse
import spfs

import structlog
from colorama import Fore, Style

import spk
import spk.external


_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    import_cmd = sub_parsers.add_parser("import", help=_import.__doc__, **parser_args)

    import_cmd.add_argument(
        "packages", metavar="FILE|NAME", nargs="+", help="The packages to import"
    )
    import_cmd.set_defaults(func=_import)
    return import_cmd


def _import(args: argparse.Namespace) -> None:
    """Import an external or previously exported package."""

    for filename in args.packages:
        spk.import_package(filename)
