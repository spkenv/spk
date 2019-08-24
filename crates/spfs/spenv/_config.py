from typing import NamedTuple
import os

from . import storage


class Config(NamedTuple):

    storage_root: str = os.path.expanduser("~/.local/share/spenv")

    def repository_storage(self) -> storage.RepositoryStorage:

        metadir = os.path.join(self.storage_root, "meta")
        os.makedirs(metadir, exist_ok=True)
        return storage.RepositoryStorage(metadir)

    def layer_storage(self) -> storage.LayerStorage:

        layersdir = os.path.join(self.storage_root, "layers")
        os.makedirs(layersdir, exist_ok=True)
        return storage.LayerStorage(layersdir)
