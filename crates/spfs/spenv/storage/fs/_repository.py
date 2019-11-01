from typing import List, Union, Dict, Iterable, Tuple, IO, Iterator
import os
import uuid
import errno
import shutil
import tarfile
import hashlib

from ... import tracking
from .. import Object, Platform, Layer
from .._registry import register_scheme
from ._platform import PlatformStorage
from ._blob import BlobStorage
from ._layer import LayerStorage
from ._tag import TagStorage


class Repository:

    _layers = "layers"
    _platforms = "platforms"
    _tags = "tags"
    _blobs = "blobs"
    dirs = (_layers, _platforms, _tags, _blobs)

    def __init__(self, root: str):

        if root.startswith("file:"):
            root = root[len("file:") :]

        self._root = root
        self.layers = LayerStorage(os.path.join(root, self._layers))
        self.platforms = PlatformStorage(os.path.join(root, self._platforms))
        self.blobs = BlobStorage(os.path.join(root, self._blobs))
        self.tags = TagStorage(os.path.join(root, self._tags))

    @property
    def root(self) -> str:
        return self._root

    def read_object(self, ref: str) -> Object:

        try:
            ref = self.tags.resolve_tag(ref).target
        except ValueError:
            pass

        try:
            return self.layers.read_layer(ref)
        except ValueError:
            pass

        try:
            return self.platforms.read_platform(ref)
        except ValueError:
            pass

        raise ValueError("Unknown ref: " + ref)

    def find_aliases(self, ref: str) -> List[str]:

        digest = self.read_object(ref).digest
        aliases = set([digest])
        for tag, target in self.tags.iter_tags():
            if target == digest:
                aliases.add(tag)
        aliases.remove(digest)
        return list(aliases)

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
