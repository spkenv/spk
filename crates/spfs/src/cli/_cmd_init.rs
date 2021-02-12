from typing import Tuple
import argparse
import os
import pty
import select
import shutil
import subprocess
import sys
import termios
import time
import tty

import sentry_sdk
from colorama import Fore

import spfs
import structlog

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    init_cmd = sub_parsers.add_parser("init-runtime")
    init_cmd.add_argument("runtime_root_dir", nargs=1)
    init_cmd.add_argument("cmd", nargs=argparse.REMAINDER)
    init_cmd.set_defaults(func=_init)


def _init(args: argparse.Namespace) -> None:
    """This is a 'hidden' command.

    This command is the entry point to new environments, and
    is executed ahead of any desired process to setup the
    environment variables and other configuration that can
    only be done from within the mount namespace.
    """

    _logger.debug("initializing runtime environment")
    runtime_root = args.runtime_root_dir[0]
    os.environ["SPFS_RUNTIME"] = runtime_root
    spfs.initialize_runtime()

    try:
        returncode = _exec_runtime_command(*args.cmd)
    finally:
        try:
            # TODO: cleanup the runtime even if the command startup fails above..
            spfs.deinitialize_runtime()
        except Exception as e:
            sentry_sdk.capture_exception(e)
            _logger.debug(f"Failed to clean up runtime: {e}")
    sys.exit(returncode)


def _exec_runtime_command(*cmd: str) -> int:

    if not len(cmd) or cmd[0] == "":
        cmd = spfs.build_interactive_shell_cmd()
        _logger.debug("starting interactive shell environment")
    else:
        cmd = spfs.build_shell_initialized_command(cmd[0], *cmd[1:])
        _logger.debug("executing runtime command")
    _logger.debug(" ".join(cmd))
    proc = subprocess.Popen(cmd)
    proc.wait()
    return proc.returncode
