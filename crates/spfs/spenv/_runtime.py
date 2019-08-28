from typing import NamedTuple, List, Optional, Sequence, Any, Tuple
import os
import errno
import subprocess

import structlog

from ._resolve import which, resolve_overlayfs_options, resolve_layers_to_packages
from ._config import get_config
from . import storage

_logger = structlog.get_logger(__name__)


class NoRuntimeError(EnvironmentError):
    def __init__(self, details: str = None) -> None:
        msg = "No active runtime"
        if details:
            msg += f": {details}"
        super(NoRuntimeError, self).__init__(msg)


def active_runtime() -> storage.Runtime:

    path = os.getenv("SPENV_RUNTIME")
    if path is None:
        raise NoRuntimeError()
    return storage.Runtime(path)


def install(*refs: str) -> None:

    runtime = active_runtime()
    config = get_config()
    repo = config.get_repository()

    # TODO: ensure packages can be installed with current stack

    packages = resolve_layers_to_packages(refs)
    for package in packages:
        runtime.append_package(package)

    overlay_args = resolve_overlayfs_options(runtime)
    _spenv_remount(overlay_args)


def _spenv_remount(overlay_args: str) -> None:

    exe = which("spenv-mount")
    if exe is None:
        raise RuntimeError("'spenv-remount' not found in PATH")
    subprocess.check_call([exe, overlay_args])
