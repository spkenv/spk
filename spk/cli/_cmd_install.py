from typing import Callable, Any
import argparse
import os
import sys
import termios

import spfs
import structlog
from colorama import Fore

import spk
from ._fmt import format_ident

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    install_cmd = sub_parsers.add_parser(
        "install", aliases=["i"], help=_install.__doc__, **parser_args,
    )
    install_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The packages to install",
    )
    install_cmd.set_defaults(func=_install)
    return install_cmd


def _install(args: argparse.Namespace) -> None:
    """install a package into spfs."""

    options = spk.api.host_options()
    solver = spk.Solver(options)
    solver.add_repository(
        spk.storage.SpFSRepository(spfs.get_config().get_repository())  # FIXME: !!
    )
    for package in args.packages:
        solver.add_request(package)

    packages = solver.solve()
    print("")
    for pkg in packages.values():
        print("\t" + format_ident(pkg))
    print("")

    if input("Do you want to continue? [y/N]: ") not in ("y", "Y"):
        print("Installation cancelled")
        sys.exit(1)

    raise NotImplementedError("TODO: Install process is not implemented")
