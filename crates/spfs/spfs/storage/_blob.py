from typing import Tuple, BinaryIO

from .. import graph, encoding


class Blob(graph.Object):
    """Blobs represent an arbitrary chunk of binary data, usually a file."""

    __fields__ = ["size"]

    def __init__(self, payload: encoding.Digest, size: int) -> None:

        self.payload = payload
        self.size = size
        super(Blob, self).__init__()

    def digest(self) -> encoding.Digest:
        return self.payload

    def child_objects(self) -> Tuple[encoding.Digest, ...]:
        """Return the child object of this one in the object DG."""
        return tuple()

    def encode(self, writer: BinaryIO) -> None:

        encoding.write_digest(writer, self.payload)
        encoding.write_int(writer, self.size)

    @classmethod
    def decode(cls, reader: BinaryIO) -> "Blob":

        return Blob(encoding.read_digest(reader), encoding.read_int(reader))


class BlobStorage:
    def __init__(self, db: graph.Database) -> None:

        self._db = db

    def has_blob(self, digest: encoding.Digest) -> bool:
        """Return true if the identified blob exists in this storage."""

        try:
            self.read_blob(digest)
        except graph.UnknownObjectError:
            return False
        except AssertionError:
            return False
        return True

    def read_blob(self, digest: encoding.Digest) -> Blob:
        """Return the blob identified by the given digest.

        Raises:
            AssertionError: if the identified object is not a blob
        """

        obj = self._db.read_object(digest)
        assert isinstance(
            obj, Blob
        ), f"Loaded object is not a blob, got: {type(obj).__name__}"
        return obj
