from typing import Iterator
import os
import hashlib

from .storage import UnknownObjectError, AmbiguousReferenceError

_FULL_DIGEST_SIZE = hashlib.sha256().digest_size


class DigestStorage:
    """Base class to manage common fs operations for digest-based storage of data.

    Mostly this class is responsible for building paths, resolving sha's and
    providing base logic related to sha shortening.
    """

    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)
        self.directory_permissions = 0o777
        makedirs_with_perms(self._root, self.directory_permissions)

    @property
    def root(self) -> str:
        """Return the root directory of this storage."""
        return self._root

    def build_digest_path(self, digest: str) -> str:
        """Build the storage path for the given object digest."""

        return os.path.join(self._root, digest[:2], digest[2:])

    def ensure_digest_base_dir(self, digest: str) -> None:
        """Create all base directories needed to store the given digest file."""

        dirname = os.path.dirname(self.build_digest_path(digest))
        makedirs_with_perms(dirname, self.directory_permissions)

    def get_digest_from_path(self, path: str) -> str:
        """Given a valid storage path, get the object digest.

        This method does not validate the path and will provide
        invalid references if given an invalid path.
        """

        path = os.path.normpath(path)
        parts = path.split(os.sep)
        return parts[-2] + parts[-1]

    def get_shortened_digest(self, digest: str) -> str:
        """Return the shortened version of the given digest."""

        return digest[:10]

    def iter_digests(self) -> Iterator[str]:
        """Iterate all the digests stored in this storage."""

        try:
            dirs = os.listdir(self._root)
        except FileNotFoundError:
            dirs = []

        for dirname in dirs:
            entries = os.listdir(os.path.join(self._root, dirname))
            for entry in entries:
                digest = dirname + entry
                try:
                    digest_bytes = bytes.fromhex(digest)
                except ValueError:
                    continue
                if len(digest_bytes) != _FULL_DIGEST_SIZE:
                    print("skip " + digest)
                    continue
                yield digest

    def resolve_full_digest_path(self, short_digest: str) -> str:
        """Given a shortened digest, resolve the full object path.

        Raises:
            UnknownObjectError: if the digest cannot be resolved
            AmbiguousReferenceError: if the digest resolves to more than one path
        """

        dirname, file_prefix = short_digest[:2], short_digest[2:]
        dirpath = os.path.join(self._root, dirname)
        if len(short_digest) == _FULL_DIGEST_SIZE:
            return os.path.join(dirpath, file_prefix)
        try:
            entries = os.listdir(dirpath)
        except FileNotFoundError:
            raise UnknownObjectError(f"Unknown ref: {short_digest}")

        options = list(filter(lambda x: x.startswith(file_prefix), entries))
        if len(options) == 0:
            raise UnknownObjectError(f"Unknown ref: {short_digest}")
        if len(options) > 1:
            raise AmbiguousReferenceError(short_digest)
        return os.path.join(dirpath, options[0])

    def resolve_full_digest(self, short_digest: str) -> str:
        """Resolve the complete object digest from a shortened one.

        Raises:
            UnknownObjectError: if the digest cannot be resolved
            AmbiguousReferenceError: if the digest resolves to more than one path
        """

        path = self.resolve_full_digest_path(short_digest)
        return self.get_digest_from_path(path)


def makedirs_with_perms(dirname: str, perms: int = 0o777) -> None:
    """Recursively create the given directory with the appropriate permissions."""

    dirnames = os.path.normpath(dirname).split(os.sep)
    for i in range(2, len(dirnames) + 1):
        dirname = os.path.join("/", *dirnames[0:i])

        try:
            # even though checking existance first is not
            # needed, it is required to trigger the automounter
            # in cases when the desired path is in that location
            if not os.path.exists(dirname):
                os.mkdir(dirname, mode=0o777)
        except FileExistsError:
            continue

        try:
            os.chmod(dirname, perms)
        except PermissionError:
            # not fatal, so it's worth allowing things to continue
            # even though it could cause permission issues later on
            pass
