# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, List
import argparse
import sys

import structlog
from colorama import Fore, Style

import spkrs
import spk
import spk.io

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    bake_cmd = sub_parsers.add_parser("bake", help=_bake.__doc__, **parser_args)
    bake_cmd.add_argument(
        "package",
        metavar="PKG",
        nargs="?",
        help="Package, or package build to show the spec file of",
    )
    _flags.add_request_flags(bake_cmd)
    _flags.add_solver_flags(bake_cmd)
    bake_cmd.set_defaults(func=_bake)
    return bake_cmd


def _bake(args: argparse.Namespace) -> None:
    """bake an executable environment from a set of requests or the current environment."""

    if args.package:
        layers = _solve_and_build_new_runtime(args)
    else:
        layers = spkrs.active_runtime().get_stack()

    for layer in layers:
        print(layer)


def _solve_and_build_new_runtime(args: argparse.Namespace) -> List[spkrs.Digest]:

    solver = _flags.get_solver_from_flags(args)
    request = _flags.parse_requests_using_flags(args, args.package)[0]
    solver.add_request(request)

    try:
        solution = solver.solve()
    except spk.SolverError as e:
        print(f"{Fore.RED}{e}{Fore.RESET}")
        if args.verbose:
            graph = solver.get_last_solve_graph()
            print(spk.io.format_solve_graph(graph, verbosity=args.verbose))
        if args.verbose == 0:
            print(
                f"{Fore.YELLOW}{Style.DIM}try '--verbose' for more info{Style.RESET_ALL}",
                file=sys.stderr,
            )
        elif args.verbose < 2:
            print(
                f"{Fore.YELLOW}{Style.DIM}try '-vv' for even more info{Style.RESET_ALL}",
                file=sys.stderr,
            )

        sys.exit(1)

    stack = []
    for _, spec, source in solution.items():
        if isinstance(source, spk.api.Spec):
            raise ValueError(
                "Cannot bake, solution requires packages that need building"
            )
        stack.append(source.get_package(spec.pkg))

    return stack
