from typing_extensions import Protocol, runtime_checkable


@runtime_checkable
class Object(Protocol):
    @property
    def ref(self) -> str:
        ...


@runtime_checkable
class LayerStorage(Protocol):
    def has_layer(self, ref: str) -> bool:
        """Return true if the identified layer exists in this repository."""
        ...


@runtime_checkable
class PlatformStorage(Protocol):
    def has_platform(self, ref: str) -> bool:
        """Return true if the identified platform exists in this repository."""
        ...


@runtime_checkable
class Repository(PlatformStorage, LayerStorage, Protocol):
    """Repostory represents a storage location for spenv data."""

    pass
