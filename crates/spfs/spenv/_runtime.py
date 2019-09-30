from typing import NamedTuple, List, Optional, Sequence, Any, Tuple, Dict, IO
import os
import json
import errno
import subprocess

import structlog

from ._resolve import which, resolve_overlayfs_options, resolve_stack_to_layers
from ._config import get_config
from ._runtime_storage import Runtime, RuntimeConfig
from . import storage

_logger = structlog.get_logger(__name__)


class NoRuntimeError(EnvironmentError):
    def __init__(self, details: str = None) -> None:
        msg = "No active runtime"
        if details:
            msg += f": {details}"
        super(NoRuntimeError, self).__init__(msg)


def active_runtime() -> Runtime:

    path = os.getenv("SPENV_RUNTIME")
    if path is None:
        raise NoRuntimeError()
    return Runtime(path)


def install(*refs: str) -> None:

    runtime = active_runtime()
    install_to(runtime, *refs)

    overlay_args = resolve_overlayfs_options(runtime)
    _spenv_remount(overlay_args)


def install_to(runtime: Runtime, *refs: str) -> Tuple[str, ...]:

    config = get_config()
    repo = config.get_repository()

    digests = []
    for ref in refs:
        digests.append(repo.read_object(ref).digest)
    for digest in digests:
        runtime.push_digest(digest)
    return runtime.get_stack()


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
