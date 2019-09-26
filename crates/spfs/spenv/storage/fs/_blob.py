from typing import IO
import os
import uuid
import shutil
import hashlib

import structlog

from ... import tracking

_CHUNK_SIZE = 1024

_logger = structlog.get_logger(__name__)


class BlobStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def open_blob(self, digest: str) -> IO[bytes]:
        """Return a handle to the blob identified by the given digest.

        Raises:
            ValueError: if the blob does not exist in this storage
        """
        filepath = os.path.join(self._root, digest)
        try:
            return open(filepath, "rb")
        except FileNotFoundError:
            raise ValueError("Unknown blob: " + digest)

    def write_blob(self, data: IO[bytes]) -> str:
        """Read the given data stream to completion, and store as a blob.

        Return the digest of the stored blob.
        """

        os.makedirs(self._root, exist_ok=True)

        hasher = hashlib.sha256()
        working_filename = "work-" + uuid.uuid1().hex
        working_filepath = os.path.join(self._root, working_filename)
        with open(working_filepath, "xb") as working_file:
            chunk = data.read(_CHUNK_SIZE)
            hasher.update(chunk)
            working_file.write(chunk)

        digest = hasher.hexdigest()
        final_filepath = os.path.join(self._root, digest)
        try:
            os.rename(working_filepath, final_filepath)
        except FileExistsError:
            _logger.debug("blob already exists", digest=digest)
            os.remove(working_filepath)

        return digest

    def commit_dir(self, dirname: str) -> tracking.Manifest:

        working_dirname = "work-" + uuid.uuid1().hex
        working_dirpath = os.path.join(self._root, working_dirname)

        _logger.info("copying file tree")
        shutil.copytree(dirname, working_dirpath, symlinks=True)

        _logger.info("computing file manifest")
        manifest = tracking.compute_manifest(working_dirpath)

        _logger.info("comitting file manifest")
        for rendered_path, entry in manifest.walk_abs(working_dirpath):

            if entry.kind is tracking.EntryKind.TREE:
                continue

            comitted_path = os.path.join(self._root, entry.digest)
            try:
                os.link(rendered_path, comitted_path)
            except FileExistsError:
                _logger.debug("file exists", digest=entry.digest)
                os.remove(rendered_path)
                os.link(comitted_path, rendered_path)

        _logger.info("comitting rendered manifest")
        rendered_dirpath = os.path.join(self._root, manifest.digest)
        try:
            os.rename(working_dirpath, rendered_dirpath)
        except FileExistsError:
            shutil.rmtree(working_dirpath)
        return manifest

    def render_manifest(self, manifest: tracking.Manifest) -> str:

        rendered_dirpath = os.path.join(self._root, manifest.digest)

        for rendered_path, entry in manifest.walk_abs(rendered_dirpath):
            if entry.kind is tracking.EntryKind.TREE:
                os.makedirs(rendered_path, exist_ok=True)
            elif entry.kind is tracking.EntryKind.BLOB:
                comitted_path = os.path.join(self._root, entry.digest)
                try:
                    os.link(comitted_path, rendered_path)
                except FileNotFoundError:
                    raise ValueError("Unknown blob: " + entry.digest)
            else:
                raise NotImplementedError(f"Unsupported entry kind: {entry.kind}")

        for rendered_path, entry in reversed(list(manifest.walk_abs(rendered_dirpath))):
            os.chmod(rendered_path, entry.mode)

        return rendered_dirpath
