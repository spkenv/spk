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

from . import _flags

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
    install_cmd.add_argument(
        "--yes",
        "-y",
        action="store_true",
        default=False,
        help="Do not prompt for confirmation, just continue",
    )
    _flags.add_repo_flags(install_cmd)
    install_cmd.set_defaults(func=_install)
    return install_cmd


def _install(args: argparse.Namespace) -> None:
    """install a package into spfs."""

    try:
        spfs.active_runtime()
    except spfs.NoRuntimeError:
        raise spfs.NoRuntimeError("maybe use 'spk env' instead?")

    options = spk.api.host_options()
    solver = spk.Solver(options)
    _flags.configure_solver_with_repo_flags(args, solver)

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
        sys.exit(1)

    if not packages:
        print(f"Nothing to do.")
        return

    print("The following packages will be modified:\n")
    requested = solver.decision_tree.root.get_requests()
    primary, tertiary = [], []
    for _, spec, _ in packages.items():
        if spec.pkg.name in requested:
            primary.append(spec)
        else:
            tertiary.append(spec)

    print("  Requested:")
    for spec in primary:
        print("    " + format_ident(spec.pkg))
    if tertiary:
        print("\n  Dependencies:")
        for spec in tertiary:
            print("    " + format_ident(spec.pkg))

    print("")

    if args.yes:
        pass
    elif input("Do you want to continue? [y/N]: ").lower() not in ("y", "yes"):
        print("Installation cancelled")
        sys.exit(1)

    spk.setup_current_runtime(packages)
