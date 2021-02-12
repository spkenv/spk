from typing_extensions import Protocol, runtime_checkable


from .. import tracking


@runtime_checkable
class ManifestStorage(Protocol):
    def has_manifest(self, digest: str) -> bool:
        """Return true if the identified manifest exists in this storage."""
        ...

    def read_manifest(self, digest: str) -> tracking.Manifest:
        """Return the manifest identified by the given digest.

        Raises:
            UnknownObjectError: if the manifest does not exist in this storage
        """
        ...

    def write_manifest(self, manifest: tracking.Manifest) -> None:
        """Write the given manifest into this storage."""
        ...

    def remove_manifest(self, digest: str) -> None:
        """Remove a manifest from this storage.

        Raises:
            UnknownObjectError: if the manifest does not exist in this storage
        """
        ...


@runtime_checkable
class ManifestViewer(Protocol):
    def render_manifest(self, manifest: tracking.Manifest) -> str:
        """Create a rendered view of the given manifest on the local disk.

        Returns:
            str: the local path to the root of the rendered manifest
        """
        ...

    def remove_rendered_manifest(self, digest: str) -> None:
        """Cleanup a previously rendered manifest from the local disk."""
        ...
