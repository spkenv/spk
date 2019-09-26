from typing_extensions import Protocol, runtime_checkable

from ._layer import LayerStorage
from ._platform import PlatformStorage
from ._blob import BlobStorage


@runtime_checkable
class Object(Protocol):
    @property
    def digest(self) -> str:
        ...


@runtime_checkable
class Repository(PlatformStorage, LayerStorage, BlobStorage, Protocol):
    """Repostory represents a storage location for spenv data."""

    def read_object(self, ref: str) -> Object:
        """Read an object of unknown type by tag or digest."""
        ...

    def write_tag(self, tag: str, digest: str) -> None:
        """Tag a known digest with another name.

        Raises:
            ValueError: if the digest refers to an unknown object
        """
        ...
