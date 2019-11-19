from typing import Tuple, Dict, Optional
import os
import errno
import subprocess

import structlog

from ._config import get_config
from ._runtime import active_runtime
from ._resolve import resolve_overlayfs_options, which
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

    overlay_args = resolve_overlayfs_options(runtime)

    spenv_exe = which("spenv")
    if not spenv_exe:
        raise RuntimeError("'spenv' not found in PATH")

    args = ("init-runtime", runtime.root, command) + args

    return _build_spenv_mount_command(overlay_args, spenv_exe, *args)


def build_shell_initialized_command(command: str, *args: str) -> Tuple[str, ...]:
    """Construct a boostrapping command for initializing through the shell.

    The returned command properly calls through a shell which sets up
    the current runtime appropriately before calling the desired command.
    """

    runtime = active_runtime()
    shell = which("bash") or which("sh")
    if not shell:
        raise RuntimeError("'sh' or 'bash' not found in PATH")

    return (shell, runtime.sh_startup_file, command) + args


def build_interactive_shell_command() -> Tuple[str, ...]:
    """Construct a boostrapping command for initializing an interactive shell.

    The returned command properly invokes a shell which sets up
    the current runtime appropriately at startup.
    """

    runtime = active_runtime()
    shell = which("bash") or which("sh")
    if not shell:
        raise RuntimeError("'sh' or 'bash' not found in PATH")

    return (shell, "--init-file", runtime.sh_startup_file)


def _build_spenv_mount_command(overlay_args: str, *command: str) -> Tuple[str, ...]:

    exe = which("spenv-mount")
    if exe is None:
        raise RuntimeError("'spenv-mount' not found in PATH")
    return (exe, overlay_args) + command
