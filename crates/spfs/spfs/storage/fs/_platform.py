from typing import Optional, List, Dict, NamedTuple, Sequence, Iterable
import os
import json
import uuid
import errno
import shutil
import hashlib

import structlog

_logger = structlog.get_logger(__name__)

from .. import Platform, UnknownObjectError
from ._layer import Layer
from ._digest_store import DigestStorage


class PlatformStorage(DigestStorage):
    def __init__(self, root: str) -> None:

        super(PlatformStorage, self).__init__(root)

    def has_platform(self, digest: str) -> bool:
        """Return true if the idetified platform exists in this storage."""

        try:
            path = self.resolve_full_digest_path(digest)
            return os.path.exists(path)
        except UnknownObjectError:
            return False
        else:
            return True

    def read_platform(self, digest: str) -> Platform:
        """Read a platform's information from this storage.

        Raises:
            ValueError: If the platform does not exist.
        """

        platform_path = self.resolve_full_digest_path(digest)
        try:
            with open(platform_path, "r", encoding="utf-8") as f:
                data = json.load(f)
            return Platform.load_dict(data)
        except OSError as e:
            if e.errno == errno.ENOENT:
                raise UnknownObjectError(f"Unknown platform: {digest}")
            raise

    def remove_platform(self, digest: str) -> None:
        """Remove a platform from this storage.

        Raises:
            ValueError: If the platform does not exist.
        """

        platform_path = self.resolve_full_digest_path(digest)
        try:
            os.remove(platform_path)
        except FileNotFoundError:
            raise UnknownObjectError(f"Unknown platform: {digest}")

    def list_platforms(self) -> List[Platform]:
        """Return a list of the current stored platforms."""

        return list(self.iter_platforms())

    def iter_platforms(self) -> Iterable[Platform]:
        """Step through each of the current stored platforms."""

        for digest in self.iter_digests():
            yield self.read_platform(digest)

    def commit_stack(self, stack: Sequence[str]) -> Platform:

        platform = Platform(stack=tuple(stack))
        self.write_platform(platform)
        return platform

    def write_platform(self, platform: Platform) -> None:
        """Store the given platform data in this storage."""

        digest = platform.digest
        self.ensure_digest_base_dir(digest)
        platform_path = self.build_digest_path(digest)
        try:
            with open(platform_path, "x", encoding="utf-8") as f:
                json.dump(platform.dump_dict(), f)
            _logger.debug("platform created", digest=digest)
        except FileExistsError:
            _logger.debug("platform already exists", digest=digest)
