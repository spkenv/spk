from typing import NamedTuple, Tuple, IO
from typing_extensions import Protocol, runtime_checkable
import hashlib

import simplejson


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
