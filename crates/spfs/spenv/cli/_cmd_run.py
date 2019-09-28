import os
import sys
import argparse

import structlog

import spenv

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    run_cmd = sub_parsers.add_parser("run", help=_run.__doc__)
    run_cmd.add_argument(
        "target",
        metavar="REF",
        nargs=1,
        help="The platform or layer to define the runtime environment",
    )
    run_cmd.add_argument("cmd", metavar="CMD", nargs=1)
    run_cmd.add_argument("args", metavar="ARGS", nargs=argparse.REMAINDER)
    run_cmd.set_defaults(func=_run)


def _run(args: argparse.Namespace) -> None:
    """Run a program in a configured environment."""

    config = spenv.get_config()
    repo = config.get_repository()

    try:
        target = repo.read_object(args.target[0])
    except ValueError:
        _logger.info(f"target does not exist locally", target=args.target[0])
        target = spenv.pull_ref(args.target[0])

    if isinstance(target, spenv.storage.fs.Runtime):
        runtime = target
    else:
        _logger.info("configuring new runtime")
        runtime = repo.runtimes.create_runtime()
        spenv.install_to(runtime, args.target[0])

    _logger.info("resolving entry process")
    cmd = spenv.build_command_for_runtime(runtime, args.cmd[0], *args.args)
    os.execv(cmd[0], cmd)
