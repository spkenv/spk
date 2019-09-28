from typing import NamedTuple, List, Optional, Sequence, Any, Tuple, Dict
import os
import errno
import subprocess

import structlog

from ._resolve import which, resolve_overlayfs_options, resolve_stack_to_layers
from ._config import get_config
from . import storage

_logger = structlog.get_logger(__name__)



class NoRuntimeError(EnvironmentError):
    def __init__(self, details: str = None) -> None:
        msg = "No active runtime"
        if details:
            msg += f": {details}"
        super(NoRuntimeError, self).__init__(msg)


def active_runtime() -> storage.fs.Runtime:

    path = os.getenv("SPENV_RUNTIME")
    if path is None:
        raise NoRuntimeError()
    return storage.fs.Runtime(path)


def install(*refs: str) -> None:

    runtime = active_runtime()
    installed_layers = install_to(runtime, *refs)

    overlay_args = resolve_overlayfs_options(runtime)
    _spenv_remount(overlay_args)


def install_to(runtime: storage.fs.Runtime, *refs: str) -> List[storage.fs.Layer]:

    config = get_config()
    repo = config.get_repository()

    layers = resolve_stack_to_layers(refs)
    for layer in layers:
        runtime.append_layer(layer)
    return layers


def _spenv_remount(overlay_args: str) -> None:

    exe = which("spenv-remount")
    if exe is None:
        raise RuntimeError("'spenv-remount' not found in PATH")
    subprocess.check_call([exe, overlay_args])
