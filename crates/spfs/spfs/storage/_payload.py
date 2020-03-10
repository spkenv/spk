from typing import BinaryIO, Iterable
import abc


from .. import encoding, graph


class PayloadStorage(metaclass=abc.ABCMeta):
    """Stores arbitrary binary data payloads using their content digest."""

    @abc.abstractmethod
    def iter_digests(self) -> Iterable[encoding.Digest]:
        """Iterate all the object in this database."""
        ...

    def has_payload(self, digest: encoding.Digest) -> bool:
        """Return true if the identified payload exists."""
        try:
            self.open_payload(digest).close()
            return True
        except graph.UnknownObjectError:
            pass
        return False

    @abc.abstractmethod
    def write_payload(self, reader: BinaryIO) -> encoding.Digest:
        """Store the contents of the given stream, returning its digest."""
        ...

    @abc.abstractmethod
    def open_payload(self, digest: encoding.Digest) -> BinaryIO:
        """Return a handle to the full content of a payload.

        Raises:
            UnknownObjectError: if the payload does not exist in this storage
        """
        ...

    @abc.abstractmethod
    def remove_payload(self, digest: encoding.Digest) -> None:
        """Remove the payload idetified by the given digest.

        Raises:
            UnknownObjectError: if the payload does not exist in this storage
        """
        ...
