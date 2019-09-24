from typing import IO
import os
import uuid
import hashlib

import structlog

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
