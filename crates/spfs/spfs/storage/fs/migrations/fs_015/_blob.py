from typing import IO, List, TYPE_CHECKING
import os
import io
import json
import stat
import uuid
import shutil
import hashlib

import structlog

from . import tracking
from .storage import UnknownObjectError
from ._digest_store import DigestStorage

_CHUNK_SIZE = 1024

_logger = structlog.get_logger(__name__)


class BlobStorage(DigestStorage):
    """Manages a local file system storage of arbitrary binary data.

    Also provides harlinked renders of file manifests for use
    in local runtimes.
    """

    def __init__(self, root: str) -> None:

        super(BlobStorage, self).__init__(root)
        self.renders = ManifestStorage(os.path.join(self._root, "renders"))

        # this default is appropriate for shared repos, but can be locked further
        # in cases where the current user will own the files, and other don't need
        # to modify the storage (0x444)
        # this is because on filesystems with protected hardlinks enabled I either
        # need to own the file or have read+write+exec access to it
        self.blob_permissions = 0o777

    def has_blob(self, digest: str) -> bool:
        """Return true if the identified blob exists in this storage."""
        try:
            self.open_blob(digest).close()
        except UnknownObjectError:
            return False
        else:
            return True

    def open_blob(self, digest: str) -> IO[bytes]:
        """Return a handle to the blob identified by the given digest.

        Raises:
            ValueError: if the blob does not exist in this storage
        """
        try:
            filepath = self.resolve_full_digest_path(digest)
            return open(filepath, "rb")
        except (FileNotFoundError, UnknownObjectError):
            raise UnknownObjectError("Unknown blob: " + digest)

    def write_blob(self, data: IO[bytes]) -> str:
        """Read the given data stream to completion, and store as a blob.

        Return the digest of the stored blob.
        """

        hasher = hashlib.sha256()
        # uuid4 is used to get around issues where a high amount of
        # multiprocessing could cause the same machine to generate
        # the same uuid because of a duplicate read of the current time
        working_filename = "work-" + uuid.uuid4().hex
        working_filepath = os.path.join(self._root, working_filename)
        with open(working_filepath, "xb") as working_file:
            chunk = data.read(_CHUNK_SIZE)
            while len(chunk) > 0:
                hasher.update(chunk)
                working_file.write(chunk)
                chunk = data.read(_CHUNK_SIZE)

        digest = hasher.hexdigest()
        self.ensure_digest_base_dir(digest)
        final_filepath = self.build_digest_path(digest)
        try:
            os.rename(working_filepath, final_filepath)
            os.chmod(final_filepath, self.blob_permissions)
        except FileExistsError:
            _logger.debug("blob already exists", digest=digest)
            os.remove(working_filepath)

        return digest

    def remove_blob(self, digest: str) -> None:
        """Remove a blob from this storage."""

        path = self.resolve_full_digest_path(digest)
        try:
            os.remove(path)
        except FileNotFoundError:
            raise UnknownObjectError("Unknown Blob: " + digest)


class ManifestStorage(DigestStorage):
    def has_manifest(self, digest: str) -> bool:
        """Return true if the identified manifest exists in this storage."""

        path = self.build_digest_path(digest)
        return os.path.exists(path + ".manifest")

    def read_manifest(self, digest: str) -> tracking.Manifest:
        """Return the manifest identified by the given digest.

        Raises:
            UnknownObjectError: if the manifest does not exist in this storage
        """
        path = self.build_digest_path(digest)
        try:
            with open(path + ".manifest") as f:
                data = json.load(f)
        except FileNotFoundError:
            raise UnknownObjectError("Unknown manifest: " + digest)
        return tracking.Manifest.load_dict(data)

    def write_manifest(self, manifest: tracking.Manifest) -> None:
        """Write the given manifest into this storage."""
        path = self.build_digest_path(manifest.digest)
        self.ensure_digest_base_dir(manifest.digest)
        try:
            with open(path + ".manifest", "w+") as f:
                json.dump(manifest.dump_dict(), f)
        except FileExistsError:
            pass

    def remove_manifest(self, digest: str) -> None:
        """Remove a manifest from this storage.

        Raises:
            UnknownObjectError: if the manifest does not exist in this storage
        """
        path = self.build_digest_path(digest)
        try:
            os.remove(path + ".manifest")
        except FileNotFoundError:
            raise UnknownObjectError("Unknown manifest: " + digest)


def _was_render_completed(render_path: str) -> bool:

    return os.path.exists(render_path + ".completed")


def _mark_render_completed(render_path: str) -> None:

    open(render_path + ".completed", "w+").close()


def _unmark_render_completed(render_path: str) -> None:

    try:
        os.remove(render_path + ".completed")
    except FileNotFoundError:
        pass


def _copy_manifest(manifest: tracking.Manifest, src_root: str, dst_root: str) -> None:
    """Copy manifest contents from one directory to another.
    """

    src_root = src_root.rstrip("/")
    dst_root = dst_root.rstrip("/")

    def get_masked_entries(dirname: str, entry_names: List[str]) -> List[str]:

        ignored = []
        manifest_path = dirname[len(src_root) :] or "/"
        for name in entry_names:
            entry_path = os.path.join(manifest_path, name)
            entry = manifest.get_path(entry_path)
            if entry.kind is tracking.EntryKind.MASK:
                ignored.append(name)
        return ignored

    shutil.copytree(
        src_root,
        dst_root,
        symlinks=True,
        ignore_dangling_symlinks=True,
        ignore=get_masked_entries,
    )


if TYPE_CHECKING:
    from .storage import BlobStorage as BS, ManifestStorage as MS, ManifestViewer as MV

    bs: BS = BlobStorage("")
    ms: MS = ManifestStorage("")
