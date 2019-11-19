import os
import sys
import argparse

import structlog
from colorama import Fore

import spenv

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    init_cmd = sub_parsers.add_parser("init-runtime", help=argparse.SUPPRESS)
    init_cmd.add_argument("runtime_root_dir", nargs=1)
    init_cmd.add_argument("cmd", nargs=argparse.REMAINDER)
    init_cmd.set_defaults(func=_init)


def _init(args: argparse.Namespace) -> None:
    """This is a hidden command.

    This command is the entry point to new environments, and
    is executed ahead of any desired process to setup the
    environment variables and other configuration that can
    only be done from within the mount namespace.
    """

    _logger.info("initializing runtime environment")
    runtime_root = args.runtime_root_dir[0]
    os.environ["SPENV_RUNTIME"] = runtime_root
    spenv.initialize_runtime()
    if not len(args.cmd) or args.cmd[0] == "":
        _logger.info("starting interactive shell environment")
        cmd = spenv.build_interactive_shell_command(*args.cmd[1:])
    else:
        _logger.info("executing runtime command")
        cmd = spenv.build_shell_initialized_command(args.cmd[0], *args.cmd[1:])
    _logger.debug(" ".join(cmd))
    os.execv(cmd[0], cmd)
