from typing import NamedTuple
import os

from . import storage


class Config(NamedTuple):

    storage_root: str = os.path.expanduser("~/.local/share/spenv")

    def repository(self) -> storage.Repository:

        return storage.ensure_repository(self.storage_root)

    def runtimes(self) -> storage.RuntimeStorage:

        repo = self.repository()
        root = os.path.join(self.storage_root, "run")
        return storage.RuntimeStorage(root, repo)
