from typing import NamedTuple
import os

from . import storage


class Config(NamedTuple):

    storage_root: str = os.path.expanduser("~/.local/share/spenv")

    def repository_storage(self) -> storage.RepositoryStorage:

        repo_storage = os.path.join(self.storage_root, "meta")
        os.makedirs(repo_storage, exist_ok=True)
        return storage.RepositoryStorage(repo_storage)
