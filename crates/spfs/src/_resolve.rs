from typing import Sequence, List, Optional, Mapping
import os
import re

import structlog

from . import storage, runtime, tracking, graph, encoding
from ._config import get_config

_LOGGER = structlog.get_logger("spfs")


def compute_manifest(ref: str) -> tracking.Manifest:

    config = get_config()
    repos: List[storage.Repository] = [config.get_repository()]
    for name in config.list_remote_names():
        try:
            repos.append(config.get_remote(name))
        except Exception as e:
            _LOGGER.warning("failed to load remote repository", remote=name)
            _LOGGER.warning(" > " + str(e))
            continue

    spec = tracking.TagSpec(ref)
    for repo in repos:
        try:
            obj = repo.read_ref(spec)
        except graph.UnknownObjectError:
            continue
        else:
            return compute_object_manifest(obj, repo)
    else:
        raise graph.UnknownReferenceError(spec)


def compute_object_manifest(
    obj: graph.Object, repo: storage.Repository = None
) -> tracking.Manifest:

    if repo is None:
        config = get_config()
        repo = config.get_repository()

    if isinstance(obj, storage.Layer):
        return repo.read_manifest(obj.manifest).unlock()
    elif isinstance(obj, storage.Platform):
        layers = resolve_stack_to_layers(obj.stack, repo)
        manifest = tracking.Manifest()
        for layer in reversed(layers):
            layer_manifest = repo.read_manifest(layer.manifest)
            manifest.update(layer_manifest.unlock())
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
        manifest = repo.read_manifest(layer.manifest)
        rendered_dir = repo.render_manifest(manifest)
        overlay_dirs.append(rendered_dir)

    return overlay_dirs


def resolve_stack_to_layers(
    stack: Sequence[encoding.Digest], repo: storage.Repository = None
) -> List[storage.Layer]:
    """Given a sequence of tags and digests, resolve to the set of underlying layers."""

    if repo is None:
        config = get_config()
        repo = config.get_repository()

    layers = []
    for ref in stack:

        entry = repo.read_ref(ref)
        if isinstance(entry, storage.Layer):
            layers.append(entry)
        elif isinstance(entry, storage.Platform):
            expanded = resolve_stack_to_layers(entry.stack, repo)
            layers.extend(expanded)
        else:
            raise NotImplementedError(
                f"Cannot resolve object into a mountable filesystem layer: {type(entry)}"
            )
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
