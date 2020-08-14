import tarfile
from typing import Union
import os
import tempfile

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

    to_transfer = {pkg}
    if pkg.build is None:
        to_transfer |= set(local_repository().list_package_builds(pkg))
        to_transfer |= set(remote_repository().list_package_builds(pkg))
    else:
        to_transfer.add(pkg.with_build(None))

    tmpdir = tempfile.TemporaryDirectory()
    with tmpdir:
        tmprepo = SpFSRepository(spfs.storage.fs.FSRepository(tmpdir.name, create=True))

        for pkg in to_transfer:
            for src_repo in (local_repository(), remote_repository()):
                try:
                    _copy_package(pkg, src_repo, tmprepo)
                    break
                except (spfs.graph.UnknownReferenceError, PackageNotFoundError):
                    continue
            else:
                raise PackageNotFoundError(pkg)

        _LOGGER.info("building archive", path=filename)
        with tarfile.open(filename, "w:bz2") as tar:
            tar.add(tmpdir.name, arcname="", recursive=True)


def import_package(filename: str) -> None:

    # spfs by default will create a new tar file if the file
    # does not exist, but we want to ensure that for importing,
    # the archive is already present
    os.stat(filename)
    tar_spfs_repo = spfs.storage.tar.TarRepository(filename)
    tmpdir = tempfile.TemporaryDirectory()
    tar = tarfile.open(filename, "r")
    with tmpdir, tar:
        _LOGGER.info("Extracting archive...")
        tar.extractall(tmpdir.name)
        archive_repo = spfs.storage.fs.FSRepository(tmpdir.name, create=True)
        local_repo = local_repository()
        for tag, _ in archive_repo.tags.iter_tags():
            _LOGGER.info("importing", ref=str(tag))
            spfs.sync_ref(str(tag), archive_repo, local_repo.as_spfs_repo())


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
