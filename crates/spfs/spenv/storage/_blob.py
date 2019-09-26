from typing import IO
from typing_extensions import Protocol, runtime_checkable


@runtime_checkable
class BlobStorage(Protocol):
    def has_blob(self, digest: str) -> bool:
        """Return true if the identified blob exists in this storage."""
        ...

    def open_blob(self, digest: str) -> IO[bytes]:
        """Return a handle to the blob identified by the given digest.

        Raises:
            ValueError: if the blob does not exist in this storage
        """
        ...

    def write_blob(self, data: IO[bytes]) -> str:
        """Read the given data stream to completion, and store as a blob.

        Return the digest of the stored blob.
        """
        ...
