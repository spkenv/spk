# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any
import argparse

import structlog

import spk


_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    import_cmd = sub_parsers.add_parser(
        "import", help=_import.__doc__, description=_import.__doc__, **parser_args
    )

    import_cmd.add_argument(
        "packages", metavar="FILE|NAME", nargs="+", help="The packages to import"
    )
    import_cmd.set_defaults(func=_import)
    return import_cmd


def _import(args: argparse.Namespace) -> None:
    """Import an external or previously exported package."""

    for filename in args.packages:
        spk.import_package(filename)
