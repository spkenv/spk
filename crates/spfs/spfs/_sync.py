from typing import Optional, List, Union
import time
import queue
import shutil
import multiprocessing
from datetime import datetime

import structlog

from . import storage, tracking, graph
from ._config import get_config

_LOGGER = structlog.get_logger(__name__)
_SYNC_LOG_UPDATE_INTERVAL_SECONDS = 2
_SYNC_WORKER_COUNT = max((1, multiprocessing.cpu_count() - 1))
_SYNC_DONE_COUNTER = multiprocessing.Value("i", 0)
_SYNC_ERROR_QUEUE: "multiprocessing.Queue[Exception]" = multiprocessing.Queue(10)
_SYNC_WORKER_POOL: Optional["multiprocessing.pool.Pool"] = None


def push_ref(ref: str, remote: Union[storage.Repository, str]) -> graph.Object:

    config = get_config()
    local = config.get_repository()
    if not isinstance(remote, storage.Repository):
        remote = config.get_remote(remote)
    return sync_ref(ref, local, remote)


def pull_ref(ref: str) -> graph.Object:
    """Pull a reference to the local repository, searching all configured remotes.

    Args:
        ref (str): The reference to localize

    Raises:
        ValueError: If the remote ref could not be found
    """

    config = get_config()
    local = config.get_repository()
    for name in config.list_remote_names():
        _LOGGER.debug("looking for ref", ref=ref, remote=name)
        try:
            remote = config.get_remote(name)
        except Exception as e:
            _LOGGER.warning("failed to load remote repository", remote=name)
            _LOGGER.warning(" > " + str(e))
            continue
        try:
            remote.read_ref(ref)
        except ValueError:
            continue
        return sync_ref(ref, remote, local)
    else:
        raise graph.UnknownReferenceError("Unknown ref: " + ref)


def sync_ref(
    ref: str, src: storage.Repository, dest: storage.Repository
) -> graph.Object:

    try:
        tag: Optional[tracking.Tag] = src.tags.resolve_tag(ref)
    except (graph.UnknownObjectError, ValueError):
        tag = None

    obj = src.read_ref(ref)
    sync_object(obj, src, dest)
    if tag is not None:
        dest.tags.push_raw_tag(tag)
    return obj


def sync_object(
    obj: graph.Object, src: storage.Repository, dest: storage.Repository
) -> None:

    if isinstance(obj, storage.Layer):
        sync_layer(obj, src, dest)
    elif isinstance(obj, storage.Platform):
        sync_platform(obj, src, dest)
    elif isinstance(obj, storage.Blob):
        _LOGGER.info("syncing blob", digest=obj.digest())
        dest.commit_blob(src.payloads.open_payload(obj.digest()))
    else:
        raise NotImplementedError("Push: Unhandled object of type: " + str(type(obj)))


def sync_platform(
    platform: storage.Platform, src: storage.Repository, dest: storage.Repository
) -> None:

    if dest.has_platform(platform.digest()):
        _LOGGER.info("platform already synced", digest=platform.digest())
        return
    _LOGGER.info("syncing platform", digest=platform.digest())
    for digest in platform.stack:
        obj = src.objects.read_object(digest)
        sync_object(obj, src, dest)

    dest.objects.write_object(platform)


def sync_layer(
    layer: storage.Layer, src: storage.Repository, dest: storage.Repository
) -> None:

    worker_pool = _get_worker_pool()
    if dest.has_layer(layer.digest()):
        _LOGGER.info("layer already synced", digest=layer.digest())
        return

    _LOGGER.info("syncing layer", digest=layer.digest())
    _SYNC_DONE_COUNTER.value = 0
    errors: List[Exception] = []
    manifest = src.read_manifest(layer.manifest)

    entries = list(
        filter(lambda e: e.kind is tracking.EntryKind.BLOB, manifest.iter_entries())
    )
    last_report = datetime.now().timestamp()
    total_count = len(entries)
    current_count = -1
    while current_count < total_count:
        current_count = _SYNC_DONE_COUNTER.value
        now = datetime.now().timestamp()

        try:
            entry = entries.pop()
        except IndexError:
            time.sleep(0.1)
        else:
            if dest.concurrent():
                worker_pool.apply_async(
                    _SYNC_ENTRY, (entry, src.address(), dest.address())
                )
            else:
                _SYNC_ENTRY_LOCAL(entry, src, dest)

        if now - last_report > _SYNC_LOG_UPDATE_INTERVAL_SECONDS:
            percent_done = (current_count / total_count) * 100
            progress_message = f"{percent_done:.02f}% ({current_count}/{total_count})"
            _LOGGER.info(f"syncing layer data...", progress=progress_message)
            last_report = now

        try:
            while True:
                errors.append(_SYNC_ERROR_QUEUE.get_nowait())
        except queue.Empty:
            pass

    if len(errors) > 0:
        raise RuntimeError(f"{errors[0]}, and {len(errors)-1} more errors during sync")

    dest.objects.write_object(manifest)
    dest.objects.write_object(layer)


def _SYNC_ENTRY(entry: storage.Entry, src_address: str, dest_address: str) -> None:
    try:
        src = storage.open_repository(src_address)
        dest = storage.open_repository(dest_address)
    except Exception as e:
        _SYNC_ERROR_QUEUE.put(e)
        with _SYNC_DONE_COUNTER.get_lock():
            # read and subsequent write are not atomic unless lock is held throughout
            _SYNC_DONE_COUNTER.value += 1
        return
    try:
        _SYNC_ENTRY_LOCAL(entry, src, dest)
    except Exception as e:
        _SYNC_ERROR_QUEUE.put(e)


def _SYNC_ENTRY_LOCAL(
    entry: storage.Entry, src: storage.Repository, dest: storage.Repository
) -> None:

    try:

        if entry.kind is not tracking.EntryKind.BLOB:
            return

        if not dest.objects.has_object(entry.object):
            blob = src.objects.read_object(entry.object)
            dest.objects.write_object(blob)

        if dest.payloads.has_payload(entry.object):
            _LOGGER.debug("blob payload already synced", digest=entry.object)
        else:
            with src.payloads.open_payload(entry.object) as payload:
                _LOGGER.debug("syncing payload", digest=entry.object)
                dest.payloads.write_payload(payload)

    finally:
        with _SYNC_DONE_COUNTER.get_lock():
            # read and subsequent write are not atomic unless lock is held throughout
            _SYNC_DONE_COUNTER.value += 1


def _get_worker_pool() -> "multiprocessing.pool.Pool":

    global _SYNC_WORKER_POOL
    if _SYNC_WORKER_POOL is None:
        _SYNC_WORKER_POOL = multiprocessing.Pool(_SYNC_WORKER_COUNT)
    return _SYNC_WORKER_POOL
