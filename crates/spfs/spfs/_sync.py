from typing import Optional, List
import time
import queue
import shutil
import multiprocessing
from datetime import datetime

import structlog

from . import storage, tracking
from ._config import get_config

_logger = structlog.get_logger(__name__)
_sync_log_update_interval_seconds = 2
_sync_worker_count = max((1, multiprocessing.cpu_count() - 1))
_sync_done_counter = multiprocessing.Value("i", 0)
_sync_error_queue: "multiprocessing.Queue[Exception]" = multiprocessing.Queue(10)
_sync_worker_pool: Optional["multiprocessing.pool.Pool"] = None


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
            remote.read_object(ref)
        except ValueError:
            continue
        return sync_ref(ref, remote, local)
    else:
        raise storage.UnknownObjectError("Unknown ref: " + ref)


def sync_ref(
    ref: str, src: storage.Repository, dest: storage.Repository
) -> storage.Object:

    obj = src.read_object(ref)
    sync_object(obj, src, dest)
    if obj.digest != ref:
        dest.push_tag(ref, obj.digest)
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

    if dest.has_platform(platform.digest):
        _logger.info("platform already synced", digest=platform.digest)
        return
    _logger.info("syncing platform", digest=platform.digest)
    for ref in platform.stack:
        sync_ref(ref, src, dest)

    dest.write_platform(platform)


def sync_layer(
    layer: storage.Layer, src: storage.Repository, dest: storage.Repository
) -> None:

    worker_pool = _get_worker_pool()
    if dest.has_layer(layer.digest):
        _logger.info("layer already synced", digest=layer.digest)
        return

    _logger.info("syncing layer", digest=layer.digest)
    spawn_count = 0
    _sync_done_counter.value = 0
    results = []
    for _, entry in layer.manifest.walk():

        result = worker_pool.apply_async(
            _sync_entry, (entry, src.address(), dest.address())
        )
        results.append(result)
        spawn_count += 1

    last_report = datetime.now().timestamp()
    current_count = _sync_done_counter.value
    errors: List[Exception] = []
    while current_count < spawn_count:
        time.sleep(0.1)
        current_count = _sync_done_counter.value
        now = datetime.now().timestamp()

        if now - last_report > _sync_log_update_interval_seconds:
            percent_done = (current_count / spawn_count) * 100
            progress_message = f"{percent_done:.02f}% ({current_count}/{spawn_count})"
            _logger.info(f"syncing layer data...", progress=progress_message)
            last_report = now

        try:
            while True:
                errors.append(_sync_error_queue.get_nowait())
        except queue.Empty:
            pass

    if len(errors) > 0:
        raise RuntimeError(f"{errors[0]}, and {len(errors)-1} more errors during sync")

    dest.write_layer(layer)


def _sync_entry(entry: tracking.Entry, src_address: str, dest_address: str) -> None:

    try:
        src = storage.open_repository(src_address)
        dest = storage.open_repository(dest_address)

        if entry.kind is not tracking.EntryKind.BLOB:
            pass
        elif dest.has_blob(entry.object):
            _logger.debug("blob already synced", digest=entry.object)
        else:
            with src.open_blob(entry.object) as blob:
                _logger.debug("syncing blob", digest=entry.object)
                dest.write_blob(blob)
    except Exception as e:
        _sync_error_queue.put(e)
    with _sync_done_counter.get_lock():
        # read and subsequent write are not atomic unless lock is held throughout
        _sync_done_counter.value += 1


def _get_worker_pool() -> "multiprocessing.pool.Pool":

    global _sync_worker_pool
    if _sync_worker_pool is None:
        _sync_worker_pool = multiprocessing.Pool(_sync_worker_count)
    return _sync_worker_pool
