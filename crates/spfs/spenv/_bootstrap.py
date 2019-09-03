from typing import NamedTuple, List, Optional, Sequence, Any, Tuple
import os
import errno
import subprocess

from ._config import get_config
from ._resolve import resolve_layers_to_packages, resolve_overlayfs_options, which
from . import storage


def build_command(command: str, *args: str) -> Tuple[str, ...]:

    config = get_config()
    repo = config.get_repository()
    runtime = repo.runtimes.create_runtime()

    return build_command_for_runtime(runtime, command, *args)


def build_command_for_runtime(
    runtime: storage.Runtime, command: str, *args: str
) -> Tuple[str, ...]:

    if not os.path.isfile(command):
        command = which(command) or command

    overlay_args = resolve_overlayfs_options(runtime)

    exe = which("spenv")
    if not exe:
        raise RuntimeError("'spenv' not found in PATH")

    args = ("init-runtime", runtime.rootdir, command) + args

    return _build_spenv_mount_command(overlay_args, exe, *args)


def _build_spenv_mount_command(overlay_args: str, *command: str) -> Tuple[str, ...]:

    exe = which("spenv-mount")
    if exe is None:
        raise RuntimeError("'spenv-mount' not found in PATH")
    return (exe, overlay_args) + command
