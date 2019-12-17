from typing import List, Tuple, IO, Iterator
import os

import semver

import spenv
from ... import tracking
from .. import Object, Platform, Layer, UnknownObjectError
from .._registry import register_scheme
from ._platform import PlatformStorage
from ._blob import BlobStorage
from ._layer import LayerStorage
from ._tag import TagStorage
from ._digest_store import DigestStorage


class Repository:

    _layers = "layers"
    _platforms = "platforms"
    _tags = "tags"
    _blobs = "blobs"
    dirs = (_layers, _platforms, _tags, _blobs)

    def __init__(self, root: str):

        if root.startswith("file:///"):
            root = root[len("file://") :]
        elif root.startswith("file:"):
            root = root[len("file:") :]

        self._root = os.path.abspath(root)
        self.layers = LayerStorage(os.path.join(root, self._layers))
        self.platforms = PlatformStorage(os.path.join(root, self._platforms))
        self.blobs = BlobStorage(os.path.join(root, self._blobs))
        self.tags = TagStorage(os.path.join(root, self._tags))

        required_version = self.min_required_version()
        if semver.compare(spenv.__version__, required_version) < 0:
            raise RuntimeError(
                f"Repository requires a newer version of spenv [{required_version}]: {self.address}"
            )

    @property
    def root(self) -> str:
        return self._root

    def address(self) -> str:
        return f"file://{self.root}"

    def has_object(self, ref: str) -> bool:

        try:
            self.read_object(ref)
        except ValueError:
            return False
        return True

    def min_required_version(self) -> str:

        version_file = os.path.join(self._root, "VERSION")
        try:
            with open(version_file, "r") as f:
                return f.read().strip()
        except FileNotFoundError:
            pass

        try:
            with open(version_file, "w+") as f:
                # versioned fs repo was introduced in v0.13.0
                f.write("0.12.0")
        except (PermissionError, FileNotFoundError):
            pass

        return "0.12.0"

    def get_shortened_digest(self, ref: str) -> str:

        store, obj = self._find_object(ref)
        return store.get_shortened_digest(obj.digest)

    def read_object(self, ref: str) -> Object:

        _, obj = self._find_object(ref)
        return obj

    def _find_object(self, ref: str) -> Tuple[DigestStorage, Object]:
        try:
            ref = self.tags.resolve_tag(ref).target
        except ValueError:
            pass

        try:
            lay = self.layers.read_layer(ref)
            return self.layers, lay
        except ValueError:
            pass

        try:
            plat = self.platforms.read_platform(ref)
            return self.platforms, plat
        except ValueError:
            pass

        raise UnknownObjectError("Unknown ref: " + ref)

    def find_aliases(self, ref: str) -> List[str]:

        aliases: List[str] = []
        digest = self.read_object(ref).digest
        for spec in self.tags.find_tags(digest):
            if spec not in aliases:
                aliases.append(spec)
        if ref != digest:
            aliases.append(digest)
            aliases.remove(ref)
        return aliases

    def resolve_tag(self, tag_spec: str) -> tracking.Tag:

        return self.tags.resolve_tag(tag_spec)

    def read_tag(self, tag: str) -> Iterator[tracking.Tag]:

        return self.tags.read_tag(tag)

    def push_tag(self, tag: str, target: str) -> tracking.Tag:

        return self.tags.push_tag(tag, target)

    def has_layer(self, digest: str) -> bool:
        """Return true if the identified layer exists in this repository."""
        try:
            self.layers.read_layer(digest)
        except ValueError:
            return False
        else:
            return True

    def read_layer(self, digest: str) -> Layer:

        return self.layers.read_layer(digest)

    def write_layer(self, layer: Layer) -> None:

        self.layers.write_layer(layer)

    def has_platform(self, digest: str) -> bool:
        """Return true if the identified platform exists in this repository."""

        try:
            self.platforms.read_platform(digest)
        except ValueError:
            return False
        else:
            return True

    def read_platform(self, digest: str) -> Platform:

        return self.platforms.read_platform(digest)

    def write_platform(self, platform: Platform) -> None:

        self.platforms.write_platform(platform)

    def has_blob(self, digest: str) -> bool:
        """Return true if the identified blob exists in this storage."""
        try:
            self.blobs.open_blob(digest).close()
        except ValueError:
            return False
        else:
            return True

    def open_blob(self, digest: str) -> IO[bytes]:
        """Return a handle to the blob identified by the given digest.

        Raises:
            ValueError: if the blob does not exist in this storage
        """
        return self.blobs.open_blob(digest)

    def write_blob(self, data: IO[bytes]) -> str:
        """Read the given data stream to completion, and store as a blob.

        Return the digest of the stored blob.
        """
        return self.blobs.write_blob(data)


def ensure_repository(path: str) -> Repository:

    os.makedirs(path, exist_ok=True, mode=0o777)
    for subdir in Repository.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True, mode=0o777)

    return Repository(path)


register_scheme("file", Repository)
register_scheme("", Repository)
