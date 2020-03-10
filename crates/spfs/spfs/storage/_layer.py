from typing import Tuple, BinaryIO, Iterable

from .. import graph, encoding, tracking


class Layer(graph.Object):
    """Layers represent a logical collection of software artifacts.

    Layers are considered completely immutable, and are
    uniquely identifyable by the computed hash of all
    relevant file and metadata.
    """

    __fields__ = ["manifest"]

    def __init__(self, manifest: encoding.Digest) -> None:

        self.manifest = manifest
        super(Layer, self).__init__()

    def child_objects(self) -> Tuple[encoding.Digest, ...]:
        """Return the child object of this one in the object DG."""
        return (self.manifest,)

    def encode(self, writer: BinaryIO) -> None:

        encoding.write_digest(writer, self.manifest)

    @classmethod
    def decode(cls, reader: BinaryIO) -> "Layer":

        return Layer(manifest=encoding.read_digest(reader))


class LayerStorage:
    def __init__(self, db: graph.Database) -> None:

        self._db = db

    def iter_layers(self) -> Iterable[Layer]:
        """Iterate the objects in this storage which are layers."""

        for obj in self._db.iter_objects():
            if isinstance(obj, Layer):
                yield obj

    def has_layer(self, digest: encoding.Digest) -> bool:
        """Return true if the identified layer exists in this storage."""

        try:
            self.read_layer(digest)
        except graph.UnknownObjectError:
            return False
        except AssertionError:
            return False
        return True

    def read_layer(self, digest: encoding.Digest) -> Layer:
        """Return the layer identified by the given digest.

        Raises:
            AssertionError: if the identified object is not a layer
        """

        obj = self._db.read_object(digest)
        assert isinstance(obj, Layer), "Loaded object is not a layer"
        return obj

    def create_layer(self, manifest: tracking.Manifest) -> Layer:
        """Create and storage a new layer for the given manifest."""

        layer = Layer(manifest=manifest.digest())
        self._db.write_object(layer)
        return layer
