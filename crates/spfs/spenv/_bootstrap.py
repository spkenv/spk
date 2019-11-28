from typing import Tuple, Dict, Optional, List
import os
import errno
import subprocess

import structlog

from ._config import get_config
from ._runtime import active_runtime
from ._resolve import resolve_overlay_dirs, which
from . import storage, runtime

_logger = structlog.get_logger(__name__)


def build_command_for_runtime(
    runtime: runtime.Runtime, command: str, *args: str
) -> Tuple[str, ...]:
    """Construct a bootstrap command.

    The returned command properly calls through the relevant spenv
    binaries and runs the desired command in an existing runtime.
    """

    if not os.path.isfile(command):
        command = which(command) or command

    overlay_dirs = resolve_overlay_dirs(runtime)

    spenv_exe = which("spenv")
    if not spenv_exe:
        raise RuntimeError("'spenv' not found in PATH")

    args = ("init-runtime", runtime.root, command) + args

    return _build_spenv_enter_command(overlay_dirs, spenv_exe, *args)


def build_shell_initialized_command(command: str, *args: str) -> Tuple[str, ...]:
    """Construct a boostrapping command for initializing through the shell.

    The returned command properly calls through a shell which sets up
    the current runtime appropriately before calling the desired command.
    """

    runtime = active_runtime()
    default_shell = which("bash") or which("sh") or ""
    desired_shell = os.environ.get("SHELL", default_shell)
    shell_name = os.path.basename(desired_shell)
    if shell_name in ("bash", "sh"):
        startup_file = runtime.sh_startup_file
    if shell_name in ("csh", "tcsh"):
        startup_file = runtime.csh_startup_file
    if not desired_shell:
        raise RuntimeError("No supported shell found")

    return (desired_shell, startup_file, command) + args


def _build_spenv_enter_command(
    overlay_dirs: List[str], *command: str
) -> Tuple[str, ...]:

    exe = which("spenv-enter")
    if exe is None:
        raise RuntimeError("'spenv-enter' not found in PATH")
    return (exe, ":".join(overlay_dirs)) + command
