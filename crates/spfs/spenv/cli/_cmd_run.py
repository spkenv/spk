import os
import sys
import argparse

import structlog

import spenv

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    run_cmd = sub_parsers.add_parser("run", help=_run.__doc__)
    run_cmd.add_argument(
        "--target",
        "-t",
        dest="targets",
        default=[],
        action="append",
        help="The platform or layer ref to define the runtime "
        "environment, and be specified more than once to "
        "build up a runtime stack",
    )
    run_cmd.add_argument("cmd", metavar="CMD", nargs=1)
    run_cmd.add_argument("args", metavar="ARGS", nargs=argparse.REMAINDER)
    run_cmd.set_defaults(func=_run)


def _run(args: argparse.Namespace) -> None:
    """Run a program in a configured environment."""

    config = spenv.get_config()
    repo = config.get_repository()
    runtimes = config.get_runtime_storage()

    for target in args.targets:
        try:
            target = repo.read_object(target)
        except ValueError:
            _logger.info(f"target does not exist locally", target=target)
            target = spenv.pull_ref(target)

    _logger.info("configuring new runtime")
    runtime = runtimes.create_runtime()
    for target in args.targets:
        spenv.install_to(runtime, target)

    _logger.info("resolving entry process")
    cmd = spenv.build_command_for_runtime(runtime, args.cmd[0], *args.args)
    os.execv(cmd[0], cmd)
