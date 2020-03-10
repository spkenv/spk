import abc
from typing import Iterable
from typing_extensions import Protocol, runtime_checkable


from .. import tracking, encoding, graph


class ManifestStorage:
    def __init__(self, db: graph.Database) -> None:

        self._db = db

    def iter_manifests(self) -> Iterable[tracking.Manifest]:
        """Iterate the objects in this storage which are manifests."""

        for obj in self._db.iter_objects():
            if isinstance(obj, tracking.Manifest):
                yield obj

    def has_manifest(self, digest: encoding.Digest) -> bool:
        """Return true if the identified manifest exists in this storage."""

        try:
            self.read_manifest(digest)
        except graph.UnknownObjectError:
            return False
        except AssertionError:
            return False
        return True

    def read_manifest(self, digest: encoding.Digest) -> tracking.Manifest:
        """Return the manifest identified by the given digest.

        Raises:
            AssertionError: if the identified object is not a manifest
        """

        obj = self._db.read_object(digest)
        assert isinstance(obj, tracking.Manifest), "Loaded object is not a manifest"
        return obj


class ManifestViewer(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def render_manifest(self, manifest: tracking.Manifest) -> str:
        """Create a rendered view of the given manifest on the local disk.

        Returns:
            str: the local path to the root of the rendered manifest
        """
        ...

    @abc.abstractmethod
    def remove_rendered_manifest(self, digest: encoding.Digest) -> None:
        """Cleanup a previously rendered manifest from the local disk."""
        ...
