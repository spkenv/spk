from typing import Sequence, List, Optional, Mapping
import os
import re

from . import storage
from ._runtime_storage import Runtime
from ._config import get_config


def resolve_overlayfs_options(runtime: Runtime) -> str:
    """Compile the overlayfs options string for the given runtime.

    This string is used by the spenv-mount and spenv-remount
    commands to appropriately render the runtimes file system under /env.
    """

    config = get_config()
    repo = config.get_repository()
    lowerdirs = [runtime.lower_dir]
    layers = resolve_stack_to_layers(runtime.get_stack())
    for layer in layers:
        rendered_dir = repo.blobs.render_manifest(layer.manifest)
        lowerdirs.append(rendered_dir)
    lowerdirs = list(reversed(lowerdirs))

    return f"lowerdir={':'.join(lowerdirs)},upperdir={runtime.upper_dir},workdir={runtime.work_dir}"


def resolve_stack_to_layers(stack: Sequence[str]) -> List[storage.fs.Layer]:
    """Given a sequence of tags and digests, resolve to the set of underlying layers."""

    config = get_config()
    repo = config.get_repository()
    layers = []
    for ref in stack:

        entry = repo.read_object(ref)
        if isinstance(entry, storage.fs.Layer):
            layers.append(entry)
        elif isinstance(entry, storage.fs.Platform):
            expanded = resolve_stack_to_layers(entry.stack)
            layers.extend(expanded)
        else:
            raise NotImplementedError(type(entry))
    return layers


def which(name: str) -> Optional[str]:

    search_paths = os.getenv("PATH", "").split(os.pathsep)
    for path in search_paths:
        filepath = os.path.join(path, name)
        if _is_exe(filepath):
            return filepath
    else:
        return None


def _is_exe(filepath: str) -> bool:

    return os.path.isfile(filepath) and os.access(filepath, os.X_OK)
