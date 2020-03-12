from typing import BinaryIO, Any, TypeVar, Type
import abc
import hashlib
import base64
import io

EncodableType = TypeVar("EncodableType", bound="Encodable")


class Encodable(metaclass=abc.ABCMeta):
    """Encodable is a type that can be binary encoded to a byte stream."""

    def digest(self) -> "Digest":
        return Digest.from_encodable(self)

    def __hash__(self) -> int:
        return hash(self.digest())

    def __eq__(self, other: Any) -> bool:

        if isinstance(other, Encodable):
            return self.digest() == other.digest()
        return super(Encodable, self).__eq__(other)

    @abc.abstractmethod
    def encode(self, writer: BinaryIO) -> None:
        """Write this object in binary format."""
        ...

    @classmethod
    @abc.abstractmethod
    def decode(cls: Type[EncodableType], reader: BinaryIO) -> EncodableType:
        """Read a previously encoded object from the given binary stream."""
        ...


class Digest(bytes):
    """Digest is the result of a hashing operation over binary data."""

    def __str__(self) -> str:
        return base64.b32encode(self).decode("ascii")

    __repr__ = __str__

    @staticmethod
    def from_string(digest_str: str) -> "Digest":

        return parse_digest(digest_str)

    @staticmethod
    def from_encodable(encodable: Encodable) -> "Digest":
        """Calculate the digest of an encodable type."""
        buffer = io.BytesIO()
        encodable.encode(buffer)
        buffer.seek(0)
        hasher = Hasher(buffer.read())
        return hasher.digest()

    def str(self) -> str:
        """Return a human readable string for this digest."""
        return str(self)


class Hasher:
    """Hasher is the hashing algorithm used for digest generation.

    All storage implementations are expected to use this
    implmentation for consitency.
    """

    __fields__ = ("_sha",)

    def __init__(self, data: bytes = b"") -> None:

        self._sha = hashlib.sha256(data)

    def __getattr__(self, name: str) -> Any:

        return getattr(self._sha, name)

    def digest(self) -> Digest:
        """Return the current digest as computed by this hasher."""

        return Digest(self._sha.digest())


DIGEST_SIZE = Hasher().digest_size
EMPTY_DIGEST = Hasher().digest()
NULL_DIGEST = Digest(b"\x00" * DIGEST_SIZE)


def parse_digest(digest_str: str) -> Digest:
    """Parse a string-digest."""
    digest_bytes = base64.b32decode(digest_str)
    assert len(digest_bytes) == DIGEST_SIZE, "Invlid digest"
    return Digest(digest_bytes)
