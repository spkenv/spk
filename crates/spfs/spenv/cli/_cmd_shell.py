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
        "target",
        metavar="REF",
        nargs="?",
        help="The platform or layer to define the runtime environment",
    )
    shell_cmd.set_defaults(func=_shell)


def _shell(args: argparse.Namespace) -> None:

    # TODO: is there a better way to determine the shell to use?
    exe = os.getenv("SHELL", "/bin/bash")

    config = spenv.get_config()
    repo = config.get_repository()
    runtimes = config.get_runtime_storage()

    if args.target:
        try:
            repo.read_object(args.target)
        except ValueError:
            _logger.info("target does not exist locally", target=args.target)
            spenv.pull_ref(args.target)

    _logger.info("configuring new runtime")
    runtime = runtimes.create_runtime()
    if args.target:
        spenv.install_to(runtime, args.target)

    _logger.info("resolving entry process")
    cmd = spenv.build_command_for_runtime(runtime, exe)
    os.execv(cmd[0], cmd)
