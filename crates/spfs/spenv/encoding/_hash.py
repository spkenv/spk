from typing import BinaryIO, Any
from typing_extensions import Protocol
import hashlib
import base64
import io


class Encodable(Protocol):
    """Encodable is a type that can be binary encoded to a byte stream."""

    def encode(self, writer: BinaryIO) -> None:
        ...

    def decode(self, reader: BinaryIO) -> None:
        ...


class Digest(bytes):
    """Digest is the result of a hashing operation over binary data."""

    def __str__(self) -> str:
        return base64.b32encode(self).decode("ascii")

    def str(self) -> str:
        """Return a human readable string or this digest."""
        return str(self)

    @staticmethod
    def from_encodable(encodable: Encodable) -> "Digest":
        """Calculate the digest of an encodable type."""
        buffer = io.BytesIO()
        encodable.encode(buffer)
        buffer.seek(0)
        hasher = Hasher(buffer.read())
        return hasher.digest()


class Hasher:
    """Hasher is the hashing algorithm used for digest generation.

    All Database implementations are expected to use this
    implmentation for consitency.
    """

    __fields__ = ("_sha",)

    def __init__(self, data: bytes = b"") -> None:

        self._sha = hashlib.sha256(data)

    def __getattr__(self, name: str) -> Any:

        return getattr(self._sha, name)

    def digest(self) -> Digest:

        return Digest(self._sha.digest())


EMPTY_DIGEST = Hasher().digest()
DIGEST_SIZE = Hasher().digest_size


def parse_digest(digest_str: str) -> Digest:
    """Parse a string-digest."""
    digest_bytes = base64.b32decode(digest_str)
    assert len(digest_bytes) == DIGEST_SIZE, "Invlid digest"
    return Digest(digest_bytes)
