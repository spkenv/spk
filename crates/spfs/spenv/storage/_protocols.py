from typing import IO, NamedTuple, Tuple, Iterable, List
from typing_extensions import Protocol, runtime_checkable
import hashlib

import simplejson


@runtime_checkable
class Object(Protocol):
    @property
    def digest(self) -> str:
        ...


class Layer(NamedTuple):
    """Layers represent a logical collection of software artifacts.

    Layers are considered completely immutable, and are
    uniquely identifyable by the computed hash of all
    relevant file and metadata.
    """

    manifest: str = ""
    environ: Tuple[str, ...] = tuple()

    @property
    def digest(self) -> str:

        hasher = hashlib.sha256()
        hasher.update(self.manifest.encode("utf-8"))
        for pair in self.environ:
            hasher.update(pair.encode("utf-8"))
        return hasher.hexdigest()

    def iter_env(self) -> Iterable[Tuple[str, str]]:

        for pair in self.environ:
            name, value = pair.split("=", 1)
            yield name, value

    def dump_json(self, stream: IO[str]) -> None:
        """Dump this config as json to the given stream."""
        simplejson.dump(self, stream)

    @staticmethod
    def load_json(stream: IO[str]) -> "Layer":
        """Load a layer data from the given json stream."""

        json_data = simplejson.load(stream)
        json_data["environ"] = tuple(json_data.get("environ", []))
        return Layer(**json_data)


@runtime_checkable
class LayerStorage(Protocol):
    def has_layer(self, digest: str) -> bool:
        """Return true if the identified layer exists in this storage."""
        ...

    def read_layer(self, digest: str) -> Layer:
        """Return the layer identified by the given digest.

        Raises:
            ValueError: if the layer does not exist in this storage
        """
        ...


class Platform(NamedTuple):
    """Platforms represent a predetermined collection of layers.

    Platforms capture an entire runtime set of layers as a single,
    identifiable object which can be applied/installed to future runtimes.
    """

    layers: Tuple[str, ...]

    @property
    def digest(self) -> str:

        hasher = hashlib.sha256()
        for layer in self.layers:
            hasher.update(layer.encode("utf-8"))
        return hasher.hexdigest()

    def dump_json(self, stream: IO[str]) -> None:
        """Dump this config as json to the given stream."""
        simplejson.dump(self, stream)

    @staticmethod
    def load_json(stream: IO[str]) -> "Platform":
        """Load a layer data from the given json stream."""

        json_data = simplejson.load(stream)
        json_data["layers"] = tuple(json_data.get("layers", []))
        return Platform(**json_data)


@runtime_checkable
class PlatformStorage(Protocol):
    def has_platform(self, ref: str) -> bool:
        """Return true if the identified platform exists in this repository."""
        ...

    def read_platform(self, ref: str) -> Platform:
        """Return the platform identified by the given ref.

        Raises:
            ValueError: if the platform does not exist in this storage
        """
        ...


@runtime_checkable
class Repository(PlatformStorage, LayerStorage, Protocol):
    """Repostory represents a storage location for spenv data."""

    pass
