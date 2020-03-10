from typing import List, Tuple, IO, Iterator, TYPE_CHECKING
import os

import semver

import spfs
from ... import tracking, graph, encoding
from .. import Platform, PlatformStorage, Layer, LayerStorage

from .. import register_scheme, Repository
from ._tag import TagStorage
from ._database import FSDatabase, FSPayloadStorage
from ._renderer import FSManifestViewer


class FSRepository(Repository, FSManifestViewer):
    def __init__(self, root: str):

        if root.startswith("file:///"):
            root = root[len("file://") :]
        elif root.startswith("file:"):
            root = root[len("file:") :]

        self.__root = os.path.abspath(root)
        self.objects = FSDatabase(os.path.join(self.__root, "objects"))
        self.payloads = FSPayloadStorage(os.path.join(self.__root, "payloads"))
        FSManifestViewer.__init__(
            self, root=os.path.join(self.__root, "renders"), payloads=self.payloads,
        )
        Repository.__init__(
            self,
            tags=TagStorage(os.path.join(self.__root, "tags")),
            object_database=self.objects,
            payload_storage=self.payloads,
        )

        self.minimum_compatible_version = "0.12.0"
        repo_version = self.last_migration()
        if semver.compare(spfs.__version__, repo_version) < 0:
            raise RuntimeError(
                f"Repository requires a newer version of spfs [{repo_version}]: {self.address()}"
            )
        if semver.compare(repo_version, self.minimum_compatible_version) < 0:
            raise RuntimeError(
                f"Repository is not compatible with this version of spfs, it needs to be migrated"
            )

    @property
    def root(self) -> str:
        return self.__root

    def address(self) -> str:
        return f"file://{self.root}"

    def last_migration(self) -> str:

        version_file = os.path.join(self.__root, "VERSION")
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
        version_file = os.path.join(self.__root, "VERSION")
        with open(version_file, "w+") as f:
            f.write(version)

    # def get_shortened_digest(self, ref: str) -> str:

    #     obj = self.read_ref(ref)
    #     return self.database.get_shortened_digest(obj.digest())

    # def read_ref(self, ref: str) -> graph.Object:

    #     try:
    #         digest = encoding.parse_digest(ref)
    #     except ValueError:
    #         digest = self.tags.resolve_tag(ref)

    #     return self.read_object(digest)
    #     pass

    # def find_aliases(self, ref: Union[str, encoding.Digest]) -> List[str]:

    #     aliases: List[str] = []
    #     digest = self.read_ref(ref).digest()
    #     for spec in self.tags.find_tags(digest):
    #         if spec not in aliases:
    #             aliases.append(spec)
    #     if ref != digest:
    #         aliases.append(digest.str())
    #         aliases.remove(ref)
    #     return aliases


def ensure_repository(path: str) -> FSRepository:

    repo = FSRepository(path)
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

    return repo


register_scheme("file", FSRepository)
register_scheme("", FSRepository)
