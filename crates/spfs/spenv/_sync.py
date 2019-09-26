import shutil

import structlog

from . import storage, tracking
from ._config import get_config

_logger = structlog.get_logger(__name__)


def push_ref(ref: str, remote_name: str) -> storage.Object:

    config = get_config()
    local = config.get_repository()
    remote = config.get_remote(remote_name)
    return sync_ref(ref, local, remote)


def pull_ref(ref: str) -> storage.Object:
    """Pull a reference to the local repository, searching all configured remotes.

    Args:
        ref (str): The reference to localize

    Raises:
        ValueError: If the remote ref could not be found
    """

    config = get_config()
    local = config.get_repository()
    for name in config.list_remote_names():
        remote = config.get_remote(name)
        try:
            return sync_ref(ref, remote, local)
        except ValueError:
            continue
    raise ValueError("Unknown ref: " + ref)


def sync_ref(
    ref: str, src: storage.Repository, dest: storage.Repository
) -> storage.Object:

    obj = src.read_object(ref)
    sync_object(obj, src, dest)
    if obj.digest != ref:
        dest.write_tag(ref, obj.digest)
    return obj


def sync_object(
    obj: storage.Object, src: storage.Repository, dest: storage.Repository
) -> None:

    if isinstance(obj, storage.Layer):
        sync_layer(obj, src, dest)
    elif isinstance(obj, storage.Platform):
        sync_platform(obj, src, dest)
    else:
        raise NotImplementedError("Push: Unhandled object of type: " + str(type(obj)))


def sync_platform(
    platform: storage.Platform, src: storage.Repository, dest: storage.Repository
) -> None:

    _logger.info("syncing platform", digest=platform.digest)
    for layer in platform.layers:
        sync_ref(layer, src, dest)

    dest.write_platform(platform)


def sync_layer(
    layer: storage.Layer, src: storage.Repository, dest: storage.Repository
) -> None:

    _logger.info("syncing layer", digest=layer.digest)

    for _, entry in layer.manifest.walk():
        if entry.kind is not tracking.EntryKind.BLOB:
            continue
        if dest.has_blob(entry.digest):
            _logger.debug("blob already exists", digest=entry.digest)
            continue
        with src.open_blob(entry.digest) as blob:
            _logger.debug("syncing blob", digest=entry.digest)
            dest.write_blob(blob)

    dest.write_layer(layer)
