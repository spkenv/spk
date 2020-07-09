from typing import Union

import spfs

from .. import api
from ._spfs import (
    local_repository,
    SpFSRepository,
    remote_repository,
    PackageNotFoundError,
)


def export_package(pkg: Union[str, api.Ident], filename: str) -> None:
    if not isinstance(pkg, api.Ident):
        pkg = api.parse_ident(pkg)

    tar_spfs_repo = spfs.storage.tar.TarRepository(filename)
    tar_repo = SpFSRepository(tar_spfs_repo)

    for src_repo in (local_repository(), remote_repository()):
        try:
            _copy_package(pkg, src_repo, tar_repo)
            return
        except (spfs.graph.UnknownReferenceError, PackageNotFoundError):
            continue

    raise PackageNotFoundError(pkg)


def import_package(filename: str) -> None:

    tar_spfs_repo = spfs.storage.tar.TarRepository(filename)
    dst_repo = local_repository().as_spfs_repo()
    for tag, _ in tar_spfs_repo.tags.iter_tags():
        spfs.sync_ref(str(tag), tar_spfs_repo, dst_repo)


def _copy_package(
    pkg: api.Ident, src_repo: SpFSRepository, dst_repo: SpFSRepository
) -> None:

    spec = src_repo.read_spec(pkg)
    digest = src_repo.get_package(pkg)

    spfs.sync_ref(digest.str(), src_repo.as_spfs_repo(), dst_repo.as_spfs_repo())
    dst_repo.publish_package(spec, digest)
