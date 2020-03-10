from typing import Iterable
import abc
import collections

from .. import encoding
from ._object import Object


class UnknownObjectError(ValueError):
    """Denotes a missing object or one that is not present in the database."""

    def __init__(self, digest: encoding.Digest) -> None:

        super(UnknownObjectError, self).__init__(f"Unknown object: {str(digest)}")


class DatabaseView(metaclass=abc.ABCMeta):
    """A read-only object database."""

    @abc.abstractmethod
    def read_object(self, digest: encoding.Digest) -> Object:
        """Read information about the given object from the database.

        Raises:
            UnknownObjectError: if the identified object does not exist in the database.
        """
        ...

    @abc.abstractmethod
    def iter_digests(self) -> Iterable[encoding.Digest]:
        """Iterate all the object digests in this database."""
        ...

    def has_object(self, digest: encoding.Digest) -> bool:

        try:
            self.read_object(digest)
        except UnknownObjectError:
            return False
        else:
            return True

    def iter_objects(self) -> Iterable[Object]:
        """Iterate all the object in this database."""

        for digest in self.iter_digests():
            yield self.read_object(digest)

    def walk_objects(self, root: encoding.Digest) -> Iterable[Object]:
        """Walk all objects connected to the given root object."""

        objs = collections.deque([self.read_object(root)])
        while len(objs) > 0:

            obj = objs.popleft()
            yield obj

            for digest in obj.child_objects():
                objs.append(self.read_object(digest))


class Database(DatabaseView):
    """Databases store and retrieve graph objects."""

    @abc.abstractmethod
    def write_object(self, obj: Object) -> None:
        """Write an object to the database, for later retrieval."""
        ...

    @abc.abstractmethod
    def remove_object(self, digest: encoding.Digest) -> None:
        """Remove an object from the database."""
        ...
