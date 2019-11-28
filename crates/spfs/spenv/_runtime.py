from typing import NamedTuple, List, Optional, Sequence, Any, Tuple, Dict, IO
import os
import json
import shutil
import errno
import subprocess

import structlog

from ._resolve import which, resolve_stack_to_layers
from ._config import get_config
from . import storage, runtime, tracking


_logger = structlog.get_logger(__name__)


class NoRuntimeError(EnvironmentError):
    def __init__(self, details: str = None) -> None:
        msg = "No active runtime"
        if details:
            msg += f": {details}"
        super(NoRuntimeError, self).__init__(msg)


def active_runtime() -> runtime.Runtime:

    path = os.getenv("SPENV_RUNTIME")
    if path is None:
        raise NoRuntimeError()
    return runtime.Runtime(path)


def initialize_runtime() -> runtime.Runtime:

    runtime = active_runtime()
    stack = runtime.get_stack()
    layers = resolve_stack_to_layers(stack)
    manifest = tracking.Manifest()
    for layer in reversed(layers):
        manifest = tracking.layer_manifests(manifest, layer.manifest)

    for path, entry in manifest.walk_abs("/env"):
        if entry.kind != tracking.EntryKind.MASK:
            continue
        _logger.debug("masking file: " + path)
        try:
            os.chmod(path, 0o777)
            os.remove(path)
        except IsADirectoryError:
            shutil.rmtree(path)
    return runtime


def deinitialize_runtime() -> None:

    runtime = active_runtime()
    runtime.delete()
    del os.environ["SPENV_RUNTIME"]


def _spenv_remount(overlay_args: str) -> None:

    exe = _which("spenv-remount")
    if exe is None:
        raise RuntimeError("'spenv-remount' not found in PATH")
    subprocess.check_call([exe, overlay_args])


def _which(name: str) -> Optional[str]:

    search_paths = os.getenv("PATH", "").split(os.pathsep)
    for path in search_paths:
        filepath = os.path.join(path, name)
        if _is_exe(filepath):
            return filepath
    else:
        return None


def _is_exe(filepath: str) -> bool:

    return os.path.isfile(filepath) and os.access(filepath, os.X_OK)
