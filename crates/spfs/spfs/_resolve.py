from typing import Sequence, List, Optional, Mapping
import os
import re

from . import storage, runtime, tracking
from ._config import get_config


def compute_manifest(ref: str) -> tracking.Manifest:

    config = get_config()
    repos: List[storage.Repository] = [config.get_repository()]
    for name in config.list_remote_names():
        repos.append(config.get_remote(name))

    spec = tracking.TagSpec(ref)
    for repo in repos:
        try:
            obj = repo.read_object(spec)
        except storage.UnknownObjectError:
            continue
        else:
            return compute_object_manifest(obj, repo)
    else:
        raise storage.UnknownObjectError(spec)


def compute_object_manifest(
    obj: storage.Object, repo: storage.Repository = None
) -> tracking.Manifest:

    if isinstance(obj, storage.Layer):
        return obj.manifest
    elif isinstance(obj, storage.Platform):
        layers = resolve_stack_to_layers(obj.stack, repo)
        manifest = tracking.Manifest()
        for layer in reversed(layers):
            manifest = tracking.layer_manifests(manifest, layer.manifest)
        return manifest
    else:
        raise NotImplementedError(
            "Resolve: Unhandled object of type: " + str(type(obj))
        )


def resolve_overlay_dirs(runtime: runtime.Runtime) -> List[str]:
    """Compile the set of directories to be overlayed for a runtime.

    These are returned as a list, from bottom to top.
    """

    config = get_config()
    repo = config.get_repository()
    overlay_dirs = []
    layers = resolve_stack_to_layers(runtime.get_stack())
    for layer in layers:
        rendered_dir = repo.blobs.render_manifest(layer.manifest)
        overlay_dirs.append(rendered_dir)

    return overlay_dirs


def resolve_stack_to_layers(
    stack: Sequence[str], repo: storage.Repository = None
) -> List[storage.fs.Layer]:
    """Given a sequence of tags and digests, resolve to the set of underlying layers."""

    if repo is None:
        config = get_config()
        repo = config.get_repository()

    layers = []
    for ref in stack:

        entry = repo.read_object(ref)
        if isinstance(entry, storage.fs.Layer):
            layers.append(entry)
        elif isinstance(entry, storage.fs.Platform):
            expanded = resolve_stack_to_layers(entry.stack, repo)
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
