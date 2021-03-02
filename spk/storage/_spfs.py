from platform import version
from typing import Iterable, Union
import io
import json
import posixpath
from functools import lru_cache

import spkrs
import structlog

from .. import api
from ._repository import Repository, PackageNotFoundError, VersionExistsError

_LOGGER = structlog.get_logger("spk.storage.spfs")


class SpFSRepository(spkrs.SpFSRepository, Repository):
    def __init__(self, path: str) -> None:
        spkrs.SpFSRepository.__init__(self, path)
        Repository.__init__(self)


local_repository = spkrs.local_repository


def remote_repository(remote: str = "origin") -> SpFSRepository:
    return spkrs.remote_repository(remote)
