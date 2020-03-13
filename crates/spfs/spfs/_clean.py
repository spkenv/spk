from typing import Set, Optional, List
import time
import queue
from datetime import datetime
import multiprocessing

import structlog

from . import tracking, storage, graph, encoding

_LOGGER = structlog.get_logger("spfs.clean")

_CLEAN_LOG_UPDATE_INTERVAL_SECONDS = 2
_CLEAN_WORKER_COUNT = max((1, multiprocessing.cpu_count() - 1))
_CLEAN_DONE_COUNTER = multiprocessing.Value("i", 0)
_CLEAN_ERROR_QUEUE: "multiprocessing.Queue[Exception]" = multiprocessing.Queue(10)
_CLEAN_WORKER_POOL: Optional["multiprocessing.pool.Pool"] = None


def clean_untagged_objects(repo: storage.Repository) -> None:

    _LOGGER.info("evaluating repository digraph...")
    unattached = get_all_unattached_objects(repo)
    if len(unattached) == 0:
        _LOGGER.info("nothing to clean!")
        return

    _LOGGER.info("removing orphaned data...")

    worker_pool = _get_worker_pool()
    spawn_count = 0
    _CLEAN_DONE_COUNTER.value = 0
    results = []
    for digest in unattached:

        result = worker_pool.apply_async(_clean_object, (repo.address(), digest))
        results.append(result)
        # TODO: this stuff below is not technically objects, and maybe belongs
        # in a higher level function
        result = worker_pool.apply_async(_clean_payload, (repo.address(), digest))
        results.append(result)
        result = worker_pool.apply_async(_clean_render, (repo.address(), digest))
        results.append(result)
        spawn_count += 3

    last_report = datetime.now().timestamp()
    current_count = _CLEAN_DONE_COUNTER.value
    errors: List[Exception] = []
    while current_count < spawn_count:
        time.sleep(0.1)
        current_count = _CLEAN_DONE_COUNTER.value
        now = datetime.now().timestamp()

        if now - last_report > _CLEAN_LOG_UPDATE_INTERVAL_SECONDS:
            percent_done = (current_count / spawn_count) * 100
            progress_message = f"{percent_done:.02f}% ({current_count}/{spawn_count})"
            _LOGGER.info(f"cleaning orphaned data...", progress=progress_message)
            last_report = now

        try:
            while True:
                errors.append(_CLEAN_ERROR_QUEUE.get_nowait())
        except queue.Empty:
            pass

    if len(errors) > 0:
        raise RuntimeError(f"{errors[0]}, and {len(errors)-1} more errors during clean")

    _LOGGER.info(f"cleaned {len(unattached)} objects")


def _clean_object(repo_addr: str, digest: encoding.Digest) -> None:

    try:
        repo = storage.open_repository(repo_addr)
        try:
            repo.objects.remove_object(digest)
            return
        except graph.UnknownObjectError:
            pass
    except Exception as e:
        _CLEAN_ERROR_QUEUE.put(e)
    finally:
        with _CLEAN_DONE_COUNTER.get_lock():
            # read and subsequent write are not atomic unless lock is held throughout
            _CLEAN_DONE_COUNTER.value += 1


def _clean_payload(repo_addr: str, digest: encoding.Digest) -> None:

    try:
        repo = storage.open_repository(repo_addr)
        try:
            repo.payloads.remove_payload(digest)
            return
        except graph.UnknownObjectError:
            pass
    except Exception as e:
        _CLEAN_ERROR_QUEUE.put(e)
    finally:
        with _CLEAN_DONE_COUNTER.get_lock():
            # read and subsequent write are not atomic unless lock is held throughout
            _CLEAN_DONE_COUNTER.value += 1


def _clean_render(repo_addr: str, digest: encoding.Digest) -> None:

    try:
        repo = storage.open_repository(repo_addr)
        assert isinstance(repo, storage.ManifestViewer)
        try:
            repo.remove_rendered_manifest(digest)
            return
        except graph.UnknownObjectError:
            pass
    except Exception as e:
        _CLEAN_ERROR_QUEUE.put(e)
    finally:
        with _CLEAN_DONE_COUNTER.get_lock():
            # read and subsequent write are not atomic unless lock is held throughout
            _CLEAN_DONE_COUNTER.value += 1


def get_all_unattached_objects(repo: storage.Repository) -> Set[encoding.Digest]:

    _LOGGER.info("evaluating repository digraph...")
    digests: Set[encoding.Digest] = set()
    for digest in repo.objects.iter_digests():
        digests.add(digest)
    return digests ^ get_all_attached_objects(repo)


def get_all_unattached_payloads(repo: storage.Repository) -> Set[encoding.Digest]:

    orphaned_payloads: Set[encoding.Digest] = set()
    for digest in repo.payloads.iter_digests():
        try:
            repo.read_blob(digest)
        except graph.UnknownObjectError:
            orphaned_payloads.add(digest)
    return orphaned_payloads


def get_all_attached_objects(repo: storage.Repository) -> Set[encoding.Digest]:

    tag_targets: Set[encoding.Digest] = set()
    for _, stream in repo.tags.iter_tag_streams():
        for tag in stream:
            tag_targets.add(tag.target)

    reachable_objects: Set[encoding.Digest] = set()
    for target in tag_targets:
        reachable_objects |= repo.objects.get_descendants(target)

    return reachable_objects


def _get_worker_pool() -> "multiprocessing.pool.Pool":

    global _CLEAN_WORKER_POOL
    if _CLEAN_WORKER_POOL is None:
        _CLEAN_WORKER_POOL = multiprocessing.Pool(_CLEAN_WORKER_COUNT)
    return _CLEAN_WORKER_POOL
