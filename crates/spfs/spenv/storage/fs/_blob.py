from typing import IO, List
import os
import io
import stat
import uuid
import shutil
import hashlib

import structlog

from ... import tracking, runtime
from .. import UnknownObjectError
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
        self._renders = DigestStorage(os.path.join(self._root, "renders"))

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

        os.makedirs(self._root, exist_ok=True)

        hasher = hashlib.sha256()
        # uuid4 is used to get around issues where a high amount of
        # multiprocessing could cause the same machine to generate
        # the same uuid because of a duplicate read of the current time
        working_filename = "work-" + uuid.uuid4().hex
        working_filepath = os.path.join(self._root, working_filename)
        with open(working_filepath, "xb") as working_file:
            chunk = data.read(_CHUNK_SIZE)
            while len(chunk):
                hasher.update(chunk)
                working_file.write(chunk)
                chunk = data.read(_CHUNK_SIZE)

        digest = hasher.hexdigest()
        final_filepath = self.build_digest_path(digest)
        try:
            os.makedirs(os.path.dirname(final_filepath), exist_ok=True)
            os.rename(working_filepath, final_filepath)
            os.chmod(final_filepath, 0o444)
        except FileExistsError:
            _logger.debug("blob already exists", digest=digest)
            os.remove(working_filepath)

        return digest

    def commit_dir(self, dirname: str) -> tracking.Manifest:
        """Commit a local file system directory to this storage.

        This collects all files to store as blobs and maintains a
        render of the manifest for use immediately.
        """

        # uuid4 is used to get around issues where a high amount of
        # multiprocessing could cause the same machine to generate
        # the same uuid because of a duplicate read of the current time
        working_dirname = "work-" + uuid.uuid4().hex
        working_dirpath = os.path.join(self._root, working_dirname)

        _logger.info("computing file manifest")
        manifest = tracking.compute_manifest(dirname)

        _logger.info("copying file tree")
        _copy_manifest(manifest, dirname, working_dirpath)

        _logger.info("committing file manifest")
        for rendered_path, entry in manifest.walk_abs(working_dirpath):

            if entry.kind is tracking.EntryKind.TREE:
                continue
            if entry.kind is tracking.EntryKind.MASK:
                continue

            committed_path = self.build_digest_path(entry.object)
            if stat.S_ISLNK(entry.mode):
                data = os.readlink(rendered_path)
                stream = io.BytesIO(data.encode("utf-8"))
                digest = self.write_blob(stream)
                assert digest == entry.object, "symlink did not match expected digest"
                continue

            try:
                os.makedirs(os.path.dirname(committed_path), exist_ok=True)
                os.rename(rendered_path, committed_path)
                os.chmod(committed_path, 0o444)
            except FileExistsError:
                _logger.debug("file exists", digest=entry.object)
                os.remove(rendered_path)

        _logger.info("committing rendered manifest")
        rendered_dirpath = self._renders.build_digest_path(manifest.digest)
        os.makedirs(os.path.dirname(rendered_dirpath), exist_ok=True)
        try:
            os.rename(working_dirpath, rendered_dirpath)
        except FileExistsError:
            shutil.rmtree(working_dirpath)
        self.render_manifest(manifest)

        return manifest

    def render_manifest(self, manifest: tracking.Manifest) -> str:
        """Create a hard-linked rendering of the given file manifest.

        Raises:
            ValueErrors: if any of the blobs in the manifest are
                not available in this storage.
        """

        rendered_dirpath = self._renders.build_digest_path(manifest.digest)
        try:
            os.makedirs(rendered_dirpath)
        except FileExistsError:
            # return rendered_dirpath
            # TODO: we should be able to bail out here,
            # but there seems to be issues where some
            # renfered mnifests are not complete after being
            # comitted - so we revalidate it just in case
            # rbottriell: I suspect it's something related
            # to renaming and not linking during the commit
            pass

        for rendered_path, entry in manifest.walk_abs(rendered_dirpath):
            if entry.kind is tracking.EntryKind.TREE:
                os.makedirs(rendered_path, exist_ok=True)
            elif entry.kind is tracking.EntryKind.MASK:
                continue
            elif entry.kind is tracking.EntryKind.BLOB:
                committed_path = self.build_digest_path(entry.object)

                if stat.S_ISLNK(entry.mode):

                    try:
                        with open(committed_path, "r") as f:
                            target = f.read()
                    except FileNotFoundError:
                        raise UnknownObjectError("Unknown blob: " + entry.object)
                    try:
                        os.symlink(target, rendered_path)
                    except FileExistsError:
                        pass
                    continue

                try:
                    os.link(committed_path, rendered_path)
                except FileExistsError:
                    pass
                except FileNotFoundError:
                    raise UnknownObjectError("Unknown blob: " + entry.object)
            else:
                raise NotImplementedError(f"Unsupported entry kind: {entry.kind}")

        for rendered_path, entry in reversed(list(manifest.walk_abs(rendered_dirpath))):
            if entry.kind is tracking.EntryKind.MASK:
                continue
            if stat.S_ISLNK(entry.mode):
                continue
            os.chmod(rendered_path, entry.mode)

        return rendered_dirpath


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
            assert entry is not None, "Detected changes during commit, aborting"
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
