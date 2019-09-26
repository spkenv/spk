from typing import NamedTuple, Tuple, Dict
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

    def dump_dict(self) -> Dict:
        """Dump this platform data into a dictionary of python basic types."""

        return {"layers": list(self.layers)}

    @staticmethod
    def load_dict(data: Dict) -> "Platform":
        """Load a platform data from the given dictionary data."""

        return Platform(layers=tuple(data.get("layers", [])))


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
