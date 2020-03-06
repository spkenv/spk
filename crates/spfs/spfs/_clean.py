from typing import Set

import structlog

from . import tracking, storage

_logger = structlog.get_logger("spfs.clean")


def clean_untagged_objects(repo: storage.fs.Repository) -> None:

    unattached = get_all_unattached_objects(repo)
    for digest in unattached:
        try:
            repo.blobs.remove_blob(digest)
            _logger.debug("removed blob", digest=digest)
            continue
        except storage.UnknownObjectError:
            pass
        try:
            repo.manifests.remove_manifest(digest)
            _logger.debug("removed manifest", digest=digest)
            if isinstance(repo.blobs, storage.ManifestViewer):
                # TODO: this should be more predictable/reliable
                repo.blobs.remove_rendered_manifest(digest)
            continue
        except storage.UnknownObjectError:
            pass
        try:
            repo.platforms.remove_platform(digest)
            _logger.info("removed platform", digest=digest)
            continue
        except storage.UnknownObjectError:
            pass
        try:
            repo.layers.remove_layer(digest)
            _logger.info("removed layer", digest=digest)
            continue
        except storage.UnknownObjectError:
            pass


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
        obj = repo.read_object(digest)
        if isinstance(obj, storage.Platform):
            for child in obj.stack:
                follow_obj(child)
        elif isinstance(obj, storage.Layer):
            reachable_objects.add(obj.manifest.digest)
            for _, child_obj in obj.manifest.walk():
                reachable_objects.add(child_obj.digest)
                reachable_objects.add(child_obj.object)
        else:
            raise NotImplementedError(f"Unhandled object {type(obj)}")

    for _, stream in repo.tags.iter_tag_streams():
        for tag in stream:
            follow_obj(tag.target)

    return reachable_objects
