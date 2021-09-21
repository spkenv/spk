# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Union
import os

import spkrs
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

    # Make filename absolute as spfs::runtime::makedirs_with_perms does not handle
    # relative paths properly.
    filename = os.path.abspath(filename)

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

    target_repo = SpFSRepository(spkrs.storage.open_tar_repository(filename, create=True))

    for pkg in to_transfer:
        for src_repo in (local_repository(), remote_repository()):
            try:
                _copy_package(pkg, src_repo, target_repo)
                break
            except (RuntimeError, PackageNotFoundError):
                continue
        else:
            raise PackageNotFoundError(pkg)

    _LOGGER.info("building archive", path=filename)
    target_repo.rs.flush()


def import_package(filename: str) -> None:

    # spfs by default will create a new tar file if the file
    # does not exist, but we want to ensure that for importing,
    # the archive is already present
    os.stat(filename)
    tar_repo = spkrs.storage.open_tar_repository(filename)
    local_repo = local_repository()
    for tag in tar_repo.ls_all_tags():
        _LOGGER.info("importing", ref=str(tag))
        tar_repo.push_ref(str(tag), local_repo.rs)


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
    src_repo.rs.push_digest(digest, dst_repo.rs)
    dst_repo.publish_package(spec, digest)
