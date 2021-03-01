from typing import Any
import argparse
import sys
import os
import glob

from ruamel import yaml
from colorama import Fore, Style
import structlog

import spkrs
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
        runtime = spkrs.get_config().get_runtime_storage().create_runtime()
        argv = sys.argv
        argv.insert(0, "--")
        argv.insert(argv.index(args.command) + 1, "--no-runtime")
        cmd = spkrs.build_command_for_runtime(runtime, *argv)
        os.execv(cmd[0], cmd)

    options = _flags.get_options_from_flags(args)
    solver = _flags.get_solver_from_flags(args)

    for request in _flags.parse_requests_using_flags(args, *requests):
        solver.add_request(request)

    try:
        generator = solver.run()
        spk.io.format_decisions(generator, sys.stdout, args.verbose)
        solution = generator.solution
    except spk.SolverError as e:
        print(spk.io.format_error(e, args.verbose), file=sys.stderr)
        sys.exit(1)

    if args.verbose > 1:
        print(spk.io.format_solution(solution, args.verbose))

    solution = spk.build_required_packages(solution)
    spk.setup_current_runtime(solution)
    os.environ.update(solution.to_environment())
    if not command:
        cmd = spkrs.build_interactive_shell_cmd()
    else:
        cmd = spkrs.build_shell_initialized_command(*command)
    os.execv(cmd[0], cmd)
