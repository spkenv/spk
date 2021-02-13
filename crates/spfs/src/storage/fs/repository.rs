import os

import semver

import spfs

from .. import register_scheme, Repository
from ._tag import TagStorage
from ._database import FSDatabase, FSPayloadStorage, makedirs_with_perms
from ._renderer import FSManifestViewer


class MigrationRequiredError(RuntimeError):
    """Denotes a repository that must be upgraded before use with this spfs version."""

    def __init__(self, current_version: str, required_version: str) -> None:
        super(MigrationRequiredError, self).__init__(
            "Repository is not compatible with this version"
            f" of spfs [{current_version} < {required_version}]"
        )


class FSRepository(Repository, FSManifestViewer):
    """A pure filesystem-based repository of spfs data."""

    def __init__(self, root: str, create: bool = False):

        if root.startswith("file:///"):
            root = root[len("file://") :]
        elif root.startswith("file:"):
            root = root[len("file:") :]

        self.__root = os.path.abspath(root)

        if not os.path.exists(self.__root) and not create:
            raise ValueError("Directory does not exist: " + self.__root)
        makedirs_with_perms(self.__root)

        if len(os.listdir(self.__root)) == 0:
            set_last_migration(self.__root, spfs.__version__)

        self.objects = FSDatabase(os.path.join(self.__root, "objects"))
        self.payloads = FSPayloadStorage(os.path.join(self.__root, "payloads"))
        FSManifestViewer.__init__(
            self,
            root=os.path.join(self.__root, "renders"),
            payloads=self.payloads,
        )
        Repository.__init__(
            self,
            tags=TagStorage(os.path.join(self.__root, "tags")),
            object_database=self.objects,
            payload_storage=self.payloads,
        )

        self.minimum_compatible_version = "0.16.0"
        repo_version = semver.VersionInfo.parse(self.last_migration())
        if repo_version.compare(spfs.__version__) > 0:
            raise RuntimeError(
                f"Repository requires a newer version of spfs [{repo_version}]: {self.address()}"
            )
        if repo_version.compare(self.minimum_compatible_version) < 0:
            raise MigrationRequiredError(
                str(repo_version), self.minimum_compatible_version
            )

    @property
    def root(self) -> str:
        return self.__root

    def concurrent(self) -> bool:
        return True

    def address(self) -> str:
        return f"file://{self.root}"

    def last_migration(self) -> str:

        return read_last_migration_version(self.__root)

    def set_last_migration(self, version: str = None) -> None:

        set_last_migration(self.__root, version)


def read_last_migration_version(root: str) -> str:
    """Read the last marked migration version for a repository root path."""

    version_file = os.path.join(root, "VERSION")
    try:
        with open(version_file, "r") as f:
            return f.read().strip()
    except FileNotFoundError:
        pass

    # versioned repo introduced in 0.13.0
    # best guess if the repo exists and it's missing
    # then it predates the creation of this file
    return "0.12.0"


def set_last_migration(root: str, version: str = None) -> None:
    """Set the last migration version of the repo with the given root directory."""

    if version is None:
        version = spfs.__version__
    version_file = os.path.join(root, "VERSION")
    with open(version_file, "w+") as f:
        f.write(version)
    try:
        os.chmod(version_file, 0o666)
    except PermissionError:
        pass


register_scheme("file", FSRepository)
register_scheme("", FSRepository)
