from typing import Tuple
import os
import shutil
import subprocess

import structlog

from ._resolve import which, resolve_stack_to_layers, resolve_overlay_dirs
from ._config import get_config
from . import runtime, tracking


_logger = structlog.get_logger(__name__)


class NoRuntimeError(EnvironmentError):
    def __init__(self, details: str = None) -> None:
        msg = "No active runtime"
        if details:
            msg += f": {details}"
        super(NoRuntimeError, self).__init__(msg)


def make_active_runtime_editable() -> None:
    """Unlock the current runtime file system so that it can be modified.

    Once modified, active changes can be committed

    Raises:
        NoRuntimeError: if there is no active runtime
        ValueError: if the active runtime is already editable
    """

    rt = active_runtime()
    if rt.is_editable():
        raise ValueError("Active runtime is already editable")

    rt.set_editable(True)
    try:
        remount_runtime(rt)
    except:
        rt.set_editable(False)
        raise


def remount_runtime(rt: runtime.Runtime) -> None:
    """Remount the given runtime as configured."""

    cmd = _build_spfs_remount_command(rt)
    proc = subprocess.Popen(cmd)
    proc.wait()
    if proc.returncode != 0:
        raise RuntimeError("Failed to re-mount runtime filesystem")


def compute_runtime_manifest(rt: runtime.Runtime) -> tracking.Manifest:
    """Calculate the file manifest for the layers in the given runtime.

    The returned manifest DOES NOT include any active changes to the runtime.
    """

    config = get_config()
    repo = config.get_repository()

    stack = rt.get_stack()
    layers = resolve_stack_to_layers(stack)
    manifest = tracking.Manifest()
    for layer in reversed(layers):
        manifest.update(repo.read_manifest(layer.manifest).unlock())
    return manifest


def active_runtime() -> runtime.Runtime:
    """Return the active runtime, or raise a NoRuntimeError."""

    path = os.getenv("SPFS_RUNTIME")
    if path is None:
        raise NoRuntimeError()
    return runtime.Runtime(path)


def initialize_runtime() -> runtime.Runtime:
    """Initialize the current spfs runtime.

    This method should only be called once at startup,
    and ensures that all masked files for this runtime
    do not show up in the rendered file system.
    """

    rt = active_runtime()

    _logger.debug("computing runtime manifest")
    manifest = compute_runtime_manifest(rt)

    _logger.debug("finding files that should be masked")
    for path, entry in manifest.walk_abs("/spfs"):
        if entry.kind != tracking.EntryKind.MASK:
            continue
        _logger.debug("masking file: " + path)
        try:
            os.chmod(path, 0o777)
            os.remove(path)
        except IsADirectoryError:
            shutil.rmtree(path)
    return rt


def deinitialize_runtime() -> None:

    rt = active_runtime()
    rt.delete()
    del os.environ["SPFS_RUNTIME"]


def _build_spfs_remount_command(rt: runtime.Runtime) -> Tuple[str, ...]:

    exe = which("spfs-enter")
    if exe is None:
        raise RuntimeError("'spfs-enter' not found in PATH")

    args = [exe, "-r"]

    overlay_dirs = resolve_overlay_dirs(rt)
    for dirpath in overlay_dirs:
        args.extend(["-d", dirpath])

    if rt.is_editable():
        args.append("-e")

    return tuple(args)
