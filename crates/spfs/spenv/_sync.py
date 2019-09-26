import shutil

import structlog

from . import storage, tracking
from ._config import get_config

_logger = structlog.get_logger(__name__)


def push_ref(ref: str, remote: storage.Repository) -> None:

    config = get_config()
    local_repo = config.get_repository()
    obj = local_repo.read_object(ref)
    push_object(obj, remote)
    if obj.digest != ref:
        push_tag(ref, remote)


def push_object(obj: storage.Object, remote: storage.Repository) -> None:

    if isinstance(obj, storage.Layer):
        push_layer(obj, remote)
    elif isinstance(obj, storage.Platform):
        push_platform(obj, remote)
    else:
        raise NotImplementedError("Push: Unhandled object of type: " + str(type(obj)))


def push_platform(platform: storage.Platform, remote: storage.Repository) -> None:

    _logger.info("pushing platform", digest=platform.digest)
    for layer in platform.layers:
        push_ref(layer, remote)


def push_layer(layer: storage.Layer, remote: storage.Repository) -> None:

    _logger.info("pushing layer", digest=layer.digest)

    config = get_config()
    local_repo = config.get_repository()

    for _, entry in layer.manifest.walk():
        if entry.kind is not tracking.EntryKind.BLOB:
            continue
        if remote.has_blob(entry.digest):
            _logger.debug("blob already exists in remote", digest=entry.digest)
            continue
        with local_repo.blobs.open_blob(entry.digest) as blob:
            _logger.debug("pushing blob", digest=entry.digest)
            remote.write_blob(blob)

    remote.write_layer(layer)


def push_tag(tag: str, remote: storage.Repository) -> None:

    raise NotImplementedError("TODO: add remote tagging semantics")
