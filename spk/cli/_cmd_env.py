from typing import Any
import argparse
import sys
import os
import glob

from ruamel import yaml
from colorama import Fore, Style
import structlog

import spfs
import spk

from spk.io import format_decision

from . import _flags

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    env_cmd = sub_parsers.add_parser(
        "env", aliases=["run"], help=_env.__doc__, **parser_args
    )
    env_cmd.add_argument(
        "args",
        metavar="[PKG@STAGE|PKG...] -- [CMD]",
        nargs=argparse.REMAINDER,
        help=(
            "The environment or packages, and optional command to run. "
            "Use '--' to separate packages from command or if no command is given "
            "spawn a new shell"
        ),
    )
    add_env_flags(env_cmd)
    env_cmd.set_defaults(func=_env)
    return env_cmd


def add_env_flags(parser: argparse.ArgumentParser) -> None:

    parser.add_argument(
        "--no-runtime",
        "-nr",
        action="store_true",
        help="Reconfigure the current spfs runtime (useful for speed and debugging)",
    )
    _flags.add_solver_flags(parser)
    _flags.add_request_flags(parser)


def _env(args: argparse.Namespace) -> None:
    """Resolve and run an environment on the fly."""

    # parse args again to get flags that might be missed
    # while using the argparse.REMAINDER flag above
    extra_parser = argparse.ArgumentParser()
    extra_parser.add_argument("--verbose", "-v", action="count", default=0)
    add_env_flags(extra_parser)

    try:
        separator = args.args.index("--")
    except ValueError:
        separator = len(args.args)
    requests = args.args[:separator]
    command = args.args[separator + 1 :] or []
    args, requests = extra_parser.parse_known_args(requests, args)

    if not args.no_runtime:
        runtime = spfs.get_config().get_runtime_storage().create_runtime()
        argv = sys.argv
        argv.insert(argv.index(args.command) + 1, "--no-runtime")
        cmd = spfs.build_command_for_runtime(runtime, *argv)
        os.execv(cmd[0], cmd)

    options = _flags.get_options_from_flags(args)
    solver = _flags.get_solver_from_flags(args)

    for request in _flags.parse_requests_using_flags(args, *requests):
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
                f"{Fore.YELLOW}{Style.DIM}try '--verbose/-v' for more info{Style.RESET_ALL}",
                file=sys.stderr,
            )
        elif args.verbose < 2:
            print(
                f"{Fore.YELLOW}{Style.DIM}try '-vv' for even more info{Style.RESET_ALL}",
                file=sys.stderr,
            )

        sys.exit(1)

    solution = spk.build_required_packages(solution)
    spk.setup_current_runtime(solution)
    os.environ.update(solution.to_environment())
    if not command:
        cmd = spfs.build_interactive_shell_cmd()
    else:
        cmd = spfs.build_shell_initialized_command(*command)
    os.execv(cmd[0], cmd)
