from typing import NamedTuple, Dict
from typing_extensions import Protocol, runtime_checkable
import hashlib


from .. import tracking


class Layer(NamedTuple):
    """Layers represent a logical collection of software artifacts.

    Layers are considered completely immutable, and are
    uniquely identifyable by the computed hash of all
    relevant file and metadata.
    """

    manifest: tracking.Manifest

    @property
    def digest(self) -> str:

        hasher = hashlib.sha256()
        hasher.update(self.manifest.digest.encode("utf-8"))
        return hasher.hexdigest()

    def dump_dict(self) -> Dict:
        """Dump this layer data into a dictionary of python basic types."""

        return {"manifest": self.manifest.dump_dict()}

    @staticmethod
    def load_dict(data: Dict) -> "Layer":
        """Load a layer data from the given dictionary data."""

        return Layer(manifest=tracking.Manifest.load_dict(data.get("manifest", {})))


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

    def write_layer(self, layer: Layer) -> None:
        """Write the given layer into this storage."""
        ...
