from typing import Dict, List
import os
import sys
import argparse

from colorama import Fore
import structlog

import spenv

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    shell_cmd = sub_parsers.add_parser("shell", help=_shell.__doc__)
    shell_cmd.add_argument(
        "--target",
        "-t",
        dest="targets",
        default=[],
        action="append",
        help="The platform or layer ref to define the runtime "
        "environment, and be specified more than once to "
        "build up a runtime stack",
    )
    shell_cmd.set_defaults(func=_shell)


def _shell(args: argparse.Namespace) -> None:

    config = spenv.get_config()
    repo = config.get_repository()
    runtimes = config.get_runtime_storage()

    for target in args.targets:
        try:
            repo.read_object(target)
        except ValueError:
            _logger.info("target does not exist locally", target=target)
            spenv.pull_ref(target)

    _logger.info("configuring new runtime")
    runtime = runtimes.create_runtime()
    for target in args.targets:
        spenv.install_to(runtime, target)

    _logger.info("resolving entry process")
    cmd = spenv.build_command_for_runtime(runtime, "")
    os.execv(cmd[0], cmd)
