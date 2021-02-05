from typing import Any
import argparse
import sys

import structlog
from ruamel import yaml
from colorama import Fore, Style

import spk.io

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    view_cmd = sub_parsers.add_parser(
        "view", aliases=["info"], help=_view.__doc__, **parser_args
    )
    view_cmd.add_argument(
        "package",
        metavar="PKG",
        nargs="?",
        help="Package, or package build to show the spec file of",
    )
    _flags.add_request_flags(view_cmd)
    _flags.add_solver_flags(view_cmd)
    view_cmd.set_defaults(func=_view)
    return view_cmd


def _view(args: argparse.Namespace) -> None:
    """view the current environment or a specific package's information."""

    if not args.package:
        _print_current_env(args)
        return

    solver = _flags.get_solver_from_flags(args)
    request = _flags.parse_requests_using_flags(args, args.package)[0]
    solver.add_request(request)
    if not isinstance(request, spk.api.PkgRequest):
        raise ValueError(f"Not a package request: {request}")

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

    for _, spec, repo in solution.items():
        if spec.pkg.name == request.pkg.name:
            print(f"{Fore.BLUE}found in:{Fore.RESET} {repo}", file=sys.stderr)
            yaml.safe_dump(spec.to_dict(), sys.stdout, default_flow_style=False)
            break
    else:
        raise RuntimeError("Internal Error: requested package was not in solution")


def _print_current_env(args: argparse.Namespace) -> None:

    solution = spk.current_env()
    print(spk.io.format_solution(solution))
