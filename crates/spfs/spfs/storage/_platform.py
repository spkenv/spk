from typing import NamedTuple, Tuple, Dict
from typing_extensions import Protocol, runtime_checkable
import hashlib

import simplejson


class Platform(NamedTuple):
    """Platforms represent a predetermined collection of layers.

    Platforms capture an entire runtime stack of layers or other platforms
    as a single, identifiable object which can be applied/installed to
    future runtimes.
    """

    stack: Tuple[str, ...]

    @property
    def digest(self) -> str:
        """Return the identifying digest hash of this object."""
        hasher = hashlib.sha256()
        for layer in self.stack:
            hasher.update(layer.encode("utf-8"))
        return hasher.hexdigest()

    def children(self) -> Tuple[str, ...]:
        """Return the child object of this one in the object DG."""
        return self.stack

    def dump_dict(self) -> Dict:
        """Dump this platform data into a dictionary of python basic types."""

        return {"stack": list(self.stack)}

    @staticmethod
    def load_dict(data: Dict) -> "Platform":
        """Load a platform data from the given dictionary data."""

        return Platform(stack=tuple(data.get("stack", [])))


@runtime_checkable
class PlatformStorage(Protocol):
    def has_platform(self, ref: str) -> bool:
        """Return true if the identified platform exists in this storage."""
        ...

    def read_platform(self, ref: str) -> Platform:
        """Return the platform identified by the given ref.

        Raises:
            ValueError: if the platform does not exist in this storage
        """
        ...

    def write_platform(self, platform: Platform) -> None:
        """Write the given platform into this storage."""
        ...

    def remove_platform(self, ref: str) -> None:
        """Remove a platform from this storage."""
        ...
