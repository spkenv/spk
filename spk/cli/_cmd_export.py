# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any
import os
import argparse

import structlog
from colorama import Fore

import spk


_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    export_cmd = sub_parsers.add_parser(
        "export", help=_export.__doc__, description=_export.__doc__, **parser_args
    )
    export_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The packages to export"
    )
    export_cmd.set_defaults(func=_export)
    return export_cmd


def _export(args: argparse.Namespace) -> None:
    """Export a package as a tar file."""

    for package in args.packages:
        pkg = spk.api.parse_ident(package)
        build = ""
        if pkg.build is not None:
            build = f"_{pkg.build.digest}"
        filename = f"{pkg.name}_{pkg.version}{build}.spk"
        try:
            spk.export_package(pkg, filename)
        except spk.storage.PackageNotFoundError:
            os.remove(filename)
            raise
        print(f"{Fore.GREEN}Created: {Fore.RESET}" + filename)
