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

import spenv
import structlog

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
    runtime = spenv.initialize_runtime()

    try:
        returncode = _exec_runtime_command(runtime, *args.cmd)
    finally:
        try:
            # TODO: cleanup the runtime even if the command startup fails above..
            spenv.deinitialize_runtime()
        except Exception as e:
            sentry_sdk.capture_exception(e)
            _logger.debug(f"Failed to clean up runtime: {e}")
    sys.exit(returncode)


def _exec_runtime_command(runtime: spenv.runtime.Runtime, *cmd: str) -> int:

    if not len(cmd) or cmd[0] == "":
        cmd = _build_interactive_shell_cmd(runtime)
        _logger.info("starting interactive shell environment")
    else:
        cmd = spenv.build_shell_initialized_command(cmd[0], *cmd[1:])
        _logger.info("executing runtime command")
    _logger.debug(" ".join(cmd))
    proc = subprocess.Popen(cmd)
    proc.wait()
    return proc.returncode


def _build_interactive_shell_cmd(runtime: spenv.runtime.Runtime) -> Tuple[str, ...]:

    shell_path = os.environ.get("SHELL", "<not-set>")
    shell_name = os.path.basename(shell_path)

    if shell_name in ("csh", "tcsh"):
        return spenv.build_shell_initialized_command(shell_path, "-f")

    if shell_name not in ("bash", "sh"):
        _logger.warning(f"current shell not supported ({shell_path}) - using bash")
        shell_path = "/usr/bin/bash"
        shell_name = "bash"
    return (shell_path, "--init-file", runtime.sh_startup_file)


def old():
    old_tty_attrs = termios.tcgetattr(sys.stdin)
    tty.setraw(sys.stdin.fileno())
    tty.setcbreak(sys.stdin.fileno())

    try:
        master_fd, slave_fd = pty.openpty()
        proc = subprocess.Popen(
            [shell_path],
            preexec_fn=os.setsid,
            stdin=slave_fd,
            stdout=slave_fd,
            stderr=slave_fd,
            universal_newlines=True,
        )

        read_read, _, _ = select.select([master_fd], [], [], 0.5)
        while master_fd in read_read:
            data_out = os.read(master_fd, 10240)
            if data_out:
                os.write(sys.stdout.fileno(), data_out)
            read_read, _, _ = select.select([master_fd], [], [], 1)

        os.write(master_fd, startup_command)
        while proc.poll() is None:
            read_ready, _, _ = select.select(
                [sys.stdin.fileno(), master_fd], [], [], 0.1
            )
            if sys.stdin.fileno() in read_ready:
                data_in = os.read(sys.stdin.fileno(), 10240)
                os.write(master_fd, data_in)
            elif master_fd in read_ready:
                data_out = os.read(master_fd, 10240)
                if data_out:
                    os.write(sys.stdout.fileno(), data_out)
    finally:
        termios.tcsetattr(sys.stdin, termios.TCSADRAIN, old_tty_attrs)

    return proc.returncode
