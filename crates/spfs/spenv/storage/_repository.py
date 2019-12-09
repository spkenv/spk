from typing import List
from typing_extensions import Protocol, runtime_checkable

from ._layer import LayerStorage
from ._platform import PlatformStorage
from ._blob import BlobStorage
from ._tag import TagStorage


class UnknownObjectError(ValueError):
    """Denotes a missing object or one that is not present in a repository."""

    pass


class AmbiguousReferenceError(ValueError):
    """Denotes a reference that could refer to more than one object in the storage."""

    def __init__(self, ref: str) -> None:
        super(AmbiguousReferenceError, self).__init__(
            f"Ambiguous reference [too short]: {ref}"
        )


@runtime_checkable
class Object(Protocol):
    @property
    def digest(self) -> str:
        ...


@runtime_checkable
class Repository(TagStorage, PlatformStorage, LayerStorage, BlobStorage, Protocol):
    """Repostory represents a storage location for spenv data."""

    def address(self) -> str:
        """Return the address of this repository."""
        ...

    def get_shortened_digest(self, digest: str) -> str:
        """Return the shortened version of the given digest."""
        ...

    def has_object(self, ref: str) -> bool:
        """Return true if the given ref is a defined object in this repo."""
        ...

    def read_object(self, ref: str) -> Object:
        """Read an object of unknown type by tag or digest."""
        ...

    def find_aliases(self, ref: str) -> List[str]:
        """Return the other identifiers that can be used for 'ref'."""
        ...
