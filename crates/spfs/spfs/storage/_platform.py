from typing import Tuple, BinaryIO, Iterable

from .. import graph, encoding


class Platform(graph.Object):
    """Platforms represent a predetermined collection of layers.

    Platforms capture an entire runtime stack of layers or other platforms
    as a single, identifiable object which can be applied/installed to
    future runtimes.
    """

    __fields__ = ["stack"]

    def __init__(self, stack: Iterable[encoding.Digest]) -> None:
        self.stack = tuple(stack)
        super(Platform, self).__init__()

    def child_objects(self) -> Tuple[encoding.Digest, ...]:
        """Return the child object of this one in the object DG."""
        return self.stack

    def encode(self, writer: BinaryIO) -> None:

        encoding.write_int(writer, len(self.stack))
        for digest in self.stack:
            encoding.write_digest(writer, digest)

    @classmethod
    def decode(cls, reader: BinaryIO) -> "Platform":

        stack = []
        num_layers = encoding.read_int(reader)
        for _ in range(num_layers):
            stack.append(encoding.read_digest(reader))
        return Platform(tuple(stack))


class PlatformStorage:
    def __init__(self, db: graph.Database) -> None:

        self._db = db

    def iter_platforms(self) -> Iterable[Platform]:
        """Iterate the objects in this storage which are platforms."""

        for obj in self._db.iter_objects():
            if isinstance(obj, Platform):
                yield obj

    def has_platform(self, digest: encoding.Digest) -> bool:
        """Return true if the identified platform exists in this storage."""

        try:
            self.read_platform(digest)
        except graph.UnknownObjectError:
            return False
        except AssertionError:
            return False
        return True

    def read_platform(self, digest: encoding.Digest) -> Platform:
        """Return the platform identified by the given digest.

        Raises:
            AssertionError: if the identified object is not a platform
        """

        obj = self._db.read_object(digest)
        assert isinstance(obj, Platform), "Loaded object is not a platform"
        return obj

    def create_platform(self, stack: Iterable[encoding.Digest]) -> Platform:
        """Create and store a platform containing the given layers.

        The layers are given bottom to top order.
        """

        platform = Platform(stack=stack)
        self._db.write_object(platform)
        return platform
