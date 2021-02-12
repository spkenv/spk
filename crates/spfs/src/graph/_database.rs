from typing import Iterable, Set
import base64
import abc
import collections

from .. import encoding
from ._object import Object


class UnknownObjectError(ValueError):
    """Denotes a missing object or one that is not present in the database."""

    def __init__(self, digest: encoding.Digest) -> None:

        super(UnknownObjectError, self).__init__(f"Unknown object: {str(digest)}")


class UnknownReferenceError(ValueError):
    """Denotes a reference that is not known."""

    pass


class AmbiguousReferenceError(ValueError):
    """Denotes a reference that could refer to more than one object in the storage."""

    def __init__(self, ref: str) -> None:
        super(AmbiguousReferenceError, self).__init__(
            f"Ambiguous reference [too short]: {ref}"
        )


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

    def get_descendants(self, root: encoding.Digest) -> Set[encoding.Digest]:
        """Return the set of all objects under the given object, recursively."""

        visited: Set[encoding.Digest] = set()

        def follow(digest: encoding.Digest) -> None:
            if digest in visited:
                return

            visited.add(digest)
            try:
                obj = self.read_object(digest)
            except UnknownObjectError:
                return

            for child in obj.child_objects():
                follow(child)

        follow(root)
        return visited

    def get_shortened_digest(self, digest: encoding.Digest) -> str:
        """Return the shortened version of the given digest.

        By default this is an O(n) operation defined by the number of objects.
        Other implemntations may provide better results.
        """

        shortest_size = 5  # creates 8 char string at base 32
        shortest = digest[:shortest_size]
        for other in self.iter_digests():
            if other[:shortest_size] != shortest:
                continue
            if other == digest:
                continue
            while other[:shortest_size] == shortest:
                shortest_size += 5
                shortest = digest[:shortest_size]
        return base64.b32encode(shortest).decode()

    def resolve_full_digest(self, short_digest: str) -> encoding.Digest:
        """Resolve the complete object digest from a shortened one.

        By default this is an O(n) operation defined by the number of objects.
        Other implemntations may provide better results.

        Raises:
            UnknownReferenceError: if the digest cannot be resolved
            AmbiguousReferenceError: if the digest could point to multiple objects
        """

        decoded = base64.b32decode(short_digest.encode())
        options = []
        for digest in self.iter_digests():
            if digest[: len(decoded)] == decoded:
                options.append(digest)

        if len(options) == 0:
            raise UnknownReferenceError(short_digest)
        if len(options) > 1:
            raise AmbiguousReferenceError(short_digest)

        return options[0]


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
