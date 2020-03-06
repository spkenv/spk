from typing import List, Tuple, IO, Iterator, TYPE_CHECKING
import os

import semver

import spfs
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
        self.manifests = self.blobs.renders
        self.tags = TagStorage(os.path.join(root, self._tags))

        required_version = self.last_migration()
        if semver.compare(spfs.__version__, required_version) < 0:
            raise RuntimeError(
                f"Repository requires a newer version of spfs [{required_version}]: {self.address()}"
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

    def last_migration(self) -> str:

        version_file = os.path.join(self._root, "VERSION")
        try:
            with open(version_file, "r") as f:
                return f.read().strip()
        except FileNotFoundError:
            pass

        # versioned repo introduced in 0.13.0
        # best guess if the repo exists and it's missing
        # then it predates the creation of this file
        return "0.12.0"

    def mark_migration_version(self, version: str = None) -> None:

        if version is None:
            version = spfs.__version__
        version_file = os.path.join(self._root, "VERSION")
        with open(version_file, "w+") as f:
            f.write(version)

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

        try:
            manifest = self.manifests.read_manifest(ref)
            return self.manifests, manifest
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


def ensure_repository(path: str) -> Repository:

    repo = Repository(path)
    try:
        # even though checking existance first is not
        # needed, it is required to trigger the automounter
        # in cases when the desired path is in that location
        if not os.path.exists(path):
            os.makedirs(repo.root, mode=0o777)
    except FileExistsError:
        pass
    else:
        repo.mark_migration_version(spfs.__version__)

    for subdir in Repository.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True, mode=0o777)

    return repo


if TYPE_CHECKING:
    from .. import Repository as R

    _: R = Repository("")

register_scheme("file", Repository)
register_scheme("", Repository)
