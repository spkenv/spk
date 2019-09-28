from typing import Tuple, Dict
import os
import errno
import subprocess

import structlog

from ._config import get_config
from ._runtime import active_runtime
from ._resolve import resolve_overlayfs_options, which
from . import storage

_logger = structlog.get_logger(__name__)


def build_command(command: str, *args: str) -> Tuple[str, ...]:

    config = get_config()
    repo = config.get_repository()
    runtime = repo.runtimes.create_runtime()

    return build_command_for_runtime(runtime, command, *args)


def build_command_for_runtime(
    runtime: storage.fs.Runtime, command: str, *args: str
) -> Tuple[str, ...]:

    if not os.path.isfile(command):
        command = which(command) or command

    overlay_args = resolve_overlayfs_options(runtime)

    spenv_exe = which("spenv")
    if not spenv_exe:
        raise RuntimeError("'spenv' not found in PATH")

    args = ("init-runtime", runtime.rootdir, command) + args

    return _build_spenv_mount_command(overlay_args, spenv_exe, *args)


def build_shell_initialized_command(command: str, *args: str) -> Tuple[str, ...]:

    runtime = active_runtime()
    shell = which("bash") or which("sh")
    if not shell:
        raise RuntimeError("'sh' or 'bash' not found in PATH")

    return (shell, "-i", runtime.sh_startup_file, command) + args


def _build_spenv_mount_command(overlay_args: str, *command: str) -> Tuple[str, ...]:

    exe = which("spenv-mount")
    if exe is None:
        raise RuntimeError("'spenv-mount' not found in PATH")
    return (exe, overlay_args) + command
