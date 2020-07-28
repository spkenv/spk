from typing import Union
import os

import spfs
import structlog

from .. import api
from ._spfs import (
    local_repository,
    SpFSRepository,
    remote_repository,
    PackageNotFoundError,
)

_LOGGER = structlog.get_logger("spk.storage")


def export_package(pkg: Union[str, api.Ident], filename: str) -> None:
    if not isinstance(pkg, api.Ident):
        pkg = api.parse_ident(pkg)

    try:
        os.remove(filename)
    except FileNotFoundError:
        pass
    tar_spfs_repo = spfs.storage.tar.TarRepository(filename)
    tar_repo = SpFSRepository(tar_spfs_repo)

    to_transfer = {pkg}
    if pkg.build is None:
        to_transfer |= set(local_repository().list_package_builds(pkg))
        to_transfer |= set(remote_repository().list_package_builds(pkg))
    else:
        to_transfer.add(pkg.with_build(None))

    for pkg in to_transfer:
        for src_repo in (local_repository(), remote_repository()):
            try:
                _copy_package(pkg, src_repo, tar_repo)
                break
            except (spfs.graph.UnknownReferenceError, PackageNotFoundError):
                continue
        else:
            raise PackageNotFoundError(pkg)


def import_package(filename: str) -> None:

    # spfs by default will create a new tar file if the file
    # does not exist, but we want to ensure that for importing,
    # the archive is already present
    os.stat(filename)
    tar_spfs_repo = spfs.storage.tar.TarRepository(filename)
    tar_repo = SpFSRepository(tar_spfs_repo)
    local_repo = local_repository()
    for tag, _ in tar_spfs_repo.tags.iter_tags():
        _LOGGER.info("importing", ref=str(tag))
        spfs.sync_ref(str(tag), tar_spfs_repo, local_repo.as_spfs_repo())


def _copy_package(
    pkg: api.Ident, src_repo: SpFSRepository, dst_repo: SpFSRepository
) -> None:

    spec = src_repo.read_spec(pkg)
    if pkg.build is None:
        _LOGGER.info("exporting", pkg=str(pkg))
        dst_repo.publish_spec(spec)
        return

    digest = src_repo.get_package(pkg)
    _LOGGER.info("exporting", pkg=str(pkg))
    spfs.sync_ref(digest.str(), src_repo.as_spfs_repo(), dst_repo.as_spfs_repo())
    dst_repo.publish_package(spec, digest)
