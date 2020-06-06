from typing import Callable, Any
import argparse
import os
import sys
import termios

import spfs
import structlog
from colorama import Fore, Style

import spk
from spk.io import format_ident

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

    try:
        runtime = spfs.active_runtime()
    except spfs.NoRuntimeError:
        raise spfs.NoRuntimeError("maybe run 'spfs shell' first?")

    options = spk.api.host_options()
    solver = spk.Solver(options)
    repo = spk.storage.SpFSRepository(spfs.get_config().get_repository())  # FIXME: !!
    solver.add_repository(repo)
    for package in args.packages:
        solver.add_request(package)

    try:
        packages = solver.solve()
    except spk.SolverError as e:
        print(f"{Fore.RED}{e}{Fore.RESET}")
        if args.verbose:
            print(spk.io.format_decision_tree(solver.decision_tree))
        else:
            print(f"{Fore.YELLOW}{Style.DIM}try '--verbose' for more info{Fore.RESET}")
        exit(1)

    print("The following packages will be modified:\n")
    for spec in packages.values():
        print("\t" + format_ident(spec.pkg))
    print("")

    if input("Do you want to continue? [y/N]: ") not in ("y", "Y"):
        print("Installation cancelled")
        sys.exit(1)

    for spec in packages.values():
        digest = repo.get_package(spec.pkg)
        runtime.push_digest(digest)

    spfs.remount_runtime(runtime)
