import os
import tarfile

import semver

import spfs

from .. import register_scheme, Repository
from ._tag import TagStorage
from ._database import TarDatabase, TarPayloadStorage


class TarRepository(Repository):
    """A pure filesystem-based repository of spfs data."""

    def __init__(self, filepath: &str):

        if filepath.startswith("tar://"):
            filepath = filepath[len("tar://") :]
        elif filepath.startswith("tar:"):
            filepath = filepath[len("tar:") :]

        self.__filepath = os.path.abspath(filepath)
        if os.path.exists(self.__filepath):
            self._tar = tarfile.open(filepath, "r")
        else:
            self._tar = tarfile.open(filepath, "w")
        self.objects = TarDatabase(self._tar)
        self.payloads = TarPayloadStorage(self._tar)
        Repository.__init__(
            self,
            tags=TagStorage(self._tar),
            object_database=self.objects,
            payload_storage=self.payloads,
        )

    def __del__(self) -> None:
        try:
            self._tar.close()
        except Exception:
            pass

    @property
    def path(self) -> str:
        return self.__filepath

    def address(self) -> str:
        return f"tar://{self.__filepath}"


register_scheme("tar", TarRepository)
