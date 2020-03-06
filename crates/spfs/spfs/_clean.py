from typing import Set, Optional, List
import time
import queue
from datetime import datetime
import multiprocessing

import structlog

from . import tracking, storage

_logger = structlog.get_logger("spfs.clean")

_clean_log_update_interval_seconds = 2
_clean_worker_count = max((1, multiprocessing.cpu_count() - 1))
_clean_done_counter = multiprocessing.Value("i", 0)
_clean_error_queue: "multiprocessing.Queue[Exception]" = multiprocessing.Queue(10)
_clean_worker_pool: Optional["multiprocessing.pool.Pool"] = None


def clean_untagged_objects(repo: storage.fs.Repository) -> None:

    _logger.info("evaluating repository digraph")
    unattached = get_all_unattached_objects(repo)
    _logger.info("removing orphaned objects")

    worker_pool = _get_worker_pool()
    spawn_count = 0
    _clean_done_counter.value = 0
    results = []
    for digest in unattached:

        result = worker_pool.apply_async(_clean_object, (repo.address(), digest))
        results.append(result)
        spawn_count += 1

    last_report = datetime.now().timestamp()
    current_count = _clean_done_counter.value
    errors: List[Exception] = []
    while current_count < spawn_count:
        time.sleep(0.1)
        current_count = _clean_done_counter.value
        now = datetime.now().timestamp()

        if now - last_report > _clean_log_update_interval_seconds:
            percent_done = (current_count / spawn_count) * 100
            progress_message = f"{percent_done:.02f}% ({current_count}/{spawn_count})"
            _logger.info(f"cleaning objects...", progress=progress_message)
            last_report = now

        try:
            while True:
                errors.append(_clean_error_queue.get_nowait())
        except queue.Empty:
            pass

    if len(errors) > 0:
        raise RuntimeError(f"{errors[0]}, and {len(errors)-1} more errors during clean")


def _clean_object(repo_addr: str, digest: str) -> None:

    repo = storage.open_repository(repo_addr)
    try:
        try:
            repo.blobs.remove_blob(digest)
            return
        except storage.UnknownObjectError:
            pass
        try:
            repo.manifests.remove_manifest(digest)
            if isinstance(repo.blobs, storage.ManifestViewer):
                # TODO: this should be more predictable/reliable
                repo.blobs.remove_rendered_manifest(digest)
            return
        except storage.UnknownObjectError:
            pass
        try:
            repo.platforms.remove_platform(digest)
            return
        except storage.UnknownObjectError:
            pass
        try:
            repo.layers.remove_layer(digest)
            return
        except storage.UnknownObjectError:
            pass
    except Exception as e:
        _clean_error_queue.put(e)
    finally:
        with _clean_done_counter.get_lock():
            # read and subsequent write are not atomic unless lock is held throughout
            _clean_done_counter.value += 1


def get_all_unattached_objects(repo: storage.fs.Repository) -> Set[str]:

    digests: Set[str] = set()
    for digest in repo.manifests.iter_digests():
        digests.add(digest)
    for digest in repo.layers.iter_digests():
        digests.add(digest)
    for digest in repo.platforms.iter_digests():
        digests.add(digest)
    for digest in repo.blobs.iter_digests():
        digests.add(digest)
    return digests ^ get_all_attached_objects(repo)


def get_all_attached_objects(repo: storage.fs.Repository) -> Set[str]:

    reachable_objects: Set[str] = set()

    def follow_obj(digest: str) -> None:

        if digest in reachable_objects:
            return

        reachable_objects.add(digest)
        try:
            obj = repo.read_object(digest)
        except storage.UnknownObjectError:
            return

        for child in obj.children():
            follow_obj(child)

    for _, stream in repo.tags.iter_tag_streams():
        for tag in stream:
            follow_obj(tag.target)

    return reachable_objects


def _get_worker_pool() -> "multiprocessing.pool.Pool":

    global _clean_worker_pool
    if _clean_worker_pool is None:
        _clean_worker_pool = multiprocessing.Pool(_clean_worker_count)
    return _clean_worker_pool
