from typing import Iterator, Dict, Type, BinaryIO
import os
import io
import uuid

import sentry_sdk
import structlog

from ... import graph, encoding
from .. import PayloadStorage

_logger = structlog.get_logger("spfs.storage.fs")
_CHUNK_SIZE = 1024


class FSPayloadStorage(PayloadStorage):
    def __init__(self, root: str) -> None:

        self.__root = os.path.abspath(root)
        self.directory_permissions = 0o777
        self.file_permissions = 0o444

    @property
    def root(self) -> str:
        """Return the root directory of this storage."""
        return self.__root

    def iter_digests(self) -> Iterator[encoding.Digest]:

        try:
            dirs = os.listdir(self.__root)
        except FileNotFoundError:
            dirs = []

        for dirname in dirs:
            entries = os.listdir(os.path.join(self.__root, dirname))
            for entry in entries:
                digest_str = dirname + entry
                yield encoding.parse_digest(digest_str)

    def write_payload(self, reader: BinaryIO) -> encoding.Digest:

        working_file = os.path.join(self.root, uuid.uuid4().hex)

        hasher = encoding.Hasher()
        self._ensure_base_dir(working_file)
        with open(working_file, "wb+") as writer:
            while True:
                chunk = reader.read(_CHUNK_SIZE)
                if not chunk:
                    break
                hasher.update(chunk)
                writer.write(chunk)
            digest = hasher.digest()

            path = self._build_digest_path(digest)
            self._ensure_base_dir(path)
            try:
                os.rename(working_file, path)
            except FileExistsError:
                os.remove(working_file)
            except Exception:
                os.remove(working_file)
                raise
            try:
                os.chmod(path, self.file_permissions)
            except Exception as e:
                # not a good enough reason to fail entirely
                sentry_sdk.capture_exception(e)
                _logger.warning(f"Failed to set payload permissions: {e}")

        return digest

    def open_payload(self, digest: encoding.Digest) -> BinaryIO:
        path = self._build_digest_path(digest)
        try:
            return open(path, "rb")
        except FileNotFoundError:
            raise graph.UnknownObjectError(digest)

    def remove_payload(self, digest: encoding.Digest) -> None:

        path = self._build_digest_path(digest)
        try:
            os.remove(path)
        except FileNotFoundError:
            raise graph.UnknownObjectError(digest)

    def _build_digest_path(self, digest: encoding.Digest) -> str:

        digest_str = str(digest)
        return os.path.join(self.__root, digest_str[:2], digest_str[2:])

    def _ensure_base_dir(self, filepath: str) -> None:

        makedirs_with_perms(os.path.dirname(filepath), self.directory_permissions)

    def get_digest_from_path(self, path: str) -> encoding.Digest:
        """Given a valid storage path, get the object digest.

        This method does not validate the path and will provide
        invalid references if given an invalid path.
        """

        path = os.path.normpath(path)
        parts = path.split(os.sep)
        return encoding.parse_digest(parts[-2] + parts[-1])

    def resolve_full_digest_path(self, short_digest: str) -> str:
        """Given a shortened digest, resolve the full object path.

        Raises:
            UnknownObjectError: if the digest cannot be resolved
            graph.AmbiguousReferenceError: if the digest resolves to more than one path
        """

        dirname, file_prefix = short_digest[:2], short_digest[2:]
        dirpath = os.path.join(self.__root, dirname)
        if len(short_digest) == encoding.DIGEST_SIZE:
            return os.path.join(dirpath, file_prefix)
        try:
            entries = os.listdir(dirpath)
        except FileNotFoundError:
            raise graph.UnknownReferenceError(f"Unknown ref: {short_digest}")

        options = list(filter(lambda x: x.startswith(file_prefix), entries))
        if len(options) == 0:
            raise graph.UnknownReferenceError(f"Unknown ref: {short_digest}")
        if len(options) > 1:
            raise graph.AmbiguousReferenceError(short_digest)
        return os.path.join(dirpath, options[0])

    def get_shortened_digest(self, digest: encoding.Digest) -> str:
        """Return the shortened version of the given digest.

        This implementation improves greatly on the base one by limiting
        the possible conflicts to a subdirectory (and subset of all digests)
        """

        filepath = self._build_digest_path(digest)
        try:
            entries = os.listdir(os.path.dirname(filepath))
        except FileNotFoundError:
            raise graph.UnknownObjectError(digest)

        digest_str = digest.str()
        shortest_size = 8
        shortest = digest_str[2:shortest_size]
        for other in entries:
            if other[:shortest_size] != shortest:
                continue
            if other == digest_str[2:]:
                continue
            while other[:shortest_size] == shortest:
                shortest_size += 8
                shortest = digest_str[2:shortest_size]
        return digest_str[:shortest_size]

    def resolve_full_digest(self, short_digest: str) -> encoding.Digest:
        """Resolve the complete object digest from a shortened one.

        Raises:
            graph.UnknownObjectError: if the digest cannot be resolved
            graph.AmbiguousReferenceError: if the digest resolves to more than one path
        """

        path = self.resolve_full_digest_path(short_digest)
        return self.get_digest_from_path(path)


def makedirs_with_perms(dirname: str, perms: int = 0o777) -> None:
    """Recursively create the given directory with the appropriate permissions."""

    dirnames = os.path.normpath(dirname).split(os.sep)
    for i in range(2, len(dirnames) + 1):
        dirname = os.path.join("/", *dirnames[0:i])

        try:
            # stat first to trigger the automounter
            # in cases when the desired path is in that location,
            # otherwise mkdir just gives permission denied
            # when the path actually already exists
            try:
                os.stat(dirname)
            except:
                pass
            os.mkdir(dirname, mode=0o777)
        except FileExistsError:
            continue

        try:
            os.chmod(dirname, perms)
        except PermissionError:
            # not fatal, so it's worth allowing things to continue
            # even though it could cause permission issues later on
            pass
