from typing import List, Union, Dict, Iterable, Tuple, IO
import os
import uuid
import errno
import shutil
import tarfile
import hashlib

from .. import Object, Platform, Layer
from .._registry import register_scheme
from ._platform import PlatformStorage, UnknownPlatformError
from ._blob import BlobStorage
from ._layer import LayerStorage
from ._runtime import RuntimeStorage, Runtime


class Repository:

    _pack = "pack"
    _plat = "plat"
    _tag = "tags"
    _run = "run"
    _blob = "blob"
    dirs = (_pack, _plat, _tag, _run, _blob)

    def __init__(self, root: str):

        if root.startswith("file:"):
            root = root[len("file:") :]

        self._root = root
        self.layers = LayerStorage(os.path.join(root, self._pack))
        self.platforms = PlatformStorage(os.path.join(root, self._plat))
        self.runtimes = RuntimeStorage(os.path.join(root, self._run))
        self.blobs = BlobStorage(os.path.join(root, self._blob))

    @property
    def root(self) -> str:
        return self._root

    def read_object(self, ref: str) -> Object:

        tag_path = os.path.join(self._root, self._tag, ref)
        try:
            with open(tag_path, "r", encoding="ascii") as f:
                ref = f.read().strip()
        except OSError as e:
            if e.errno == errno.ENOENT:
                pass
            else:
                raise

        try:
            return self.layers.read_layer(ref)
        except ValueError:
            pass

        try:
            return self.platforms.read_platform(ref)
        except UnknownPlatformError:
            pass

        raise ValueError("Unknown ref: " + ref)

    def find_aliases(self, ref: str) -> List[str]:

        digest = self.read_object(ref).digest
        aliases = set([digest])
        for tag, target in self.iter_tags():
            if target == digest:
                aliases.add(tag)
        aliases.remove(digest)
        return list(aliases)

    def iter_tags(self) -> Iterable[Tuple[str, str]]:

        tag_dir = os.path.join(self._root, self._tag)
        for root, _, files in os.walk(tag_dir):

            for filename in files:
                linkfile = os.path.join(root, filename)
                with open(linkfile, "r", encoding="ascii") as f:
                    ref = f.read().strip()
                tag = os.path.relpath(linkfile, tag_dir)
                yield (tag, ref)

    def commit_layer(self, runtime: Runtime, env: Dict[str, str] = None) -> Layer:
        """Commit the working file changes of a runtime to a new layer."""

        manifest = self.blobs.commit_dir(runtime.upperdir)
        return self.layers.commit_manifest(manifest, env=env)

    def commit_platform(self, runtime: Runtime, env: Dict[str, str] = None) -> Platform:
        """Commit the full layer stack and working files to a new platform."""

        top_layer = self.commit_layer(runtime, env)
        runtime.append_layer(top_layer)
        return self.platforms.commit_runtime(runtime)

    def write_tag(self, tag: str, digest: str) -> None:

        obj = self.read_object(digest)
        tagdir = os.path.join(self._root, self._tag)
        linkfile = os.path.join(tagdir, tag)
        os.makedirs(os.path.dirname(linkfile), exist_ok=True)
        with open(linkfile, "w+", encoding="ascii") as f:
            f.write(obj.digest)

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
