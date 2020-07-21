from typing import Any
import argparse
import sys
import os

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

    env_cmd = sub_parsers.add_parser("env", help=_env.__doc__, **parser_args)
    env_cmd.add_argument(
        "args",
        metavar="[@NAME|PKG...] -- [CMD]",
        nargs=argparse.REMAINDER,
        help=(
            "The environment or packages, and optional command to run. "
            "Use '--' to separate packages from command or if no command is given "
            "spawn a new shell"
        ),
    )
    _flags.add_request_flags(env_cmd)
    _flags.add_repo_flags(env_cmd)
    _flags.add_option_flags(env_cmd)
    env_cmd.set_defaults(func=_env)
    return env_cmd


def _env(args: argparse.Namespace) -> None:
    """Resolve and run an environment on the fly."""

    try:
        separator = args.args.index("--")
    except ValueError:
        separator = len(args.args)
    requests = args.args[:separator]
    command = args.args[separator + 1 :] or [os.getenv("SHELL", "/bin/bash")]

    if not requests and os.path.exists(".spk-env.yaml"):
        _LOGGER.info("using local default environment from .spk-env.yaml")
        requests = ["@default"]

    options = _flags.get_options_from_flags(args)
    solver = spk.Solver(options)
    _flags.configure_solver_with_repo_flags(args, solver)

    has_named_env = any(r.startswith("@") for r in requests)
    if has_named_env:
        if len(requests) != 1:
            print(
                f"{Fore.RED}You can only specify single named environment{Fore.RESET}",
                file=sys.stderr,
            )
        env_name = requests[0].lstrip("@")
        env_spec = spk.load_env(env_name)
        for request in env_spec.requirements:
            solver.add_request(request)

    else:
        for request in _flags.parse_requests_using_flags(args, *requests):
            solver.add_request(request)

    try:
        solution = solver.solve()
    except spk.SolverError as e:
        print(f"{Fore.RED}{e}{Fore.RESET}")
        if args.verbose:
            print(
                spk.io.format_decision_tree(
                    solver.decision_tree, verbosity=args.verbose
                )
            )
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

    runtime = spk.create_runtime(solution)
    os.environ.update(solution.to_environment())
    os.environ.update(options.to_environment())
    cmd = spfs.build_command_for_runtime(runtime, *command)
    os.execvp(cmd[0], cmd)
