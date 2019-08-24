from typing import NamedTuple, Tuple, List
import os
import enum
import uuid
import stat
import errno
import shutil
import hashlib

from .. import tracking
from ._mount import Mount
from ._runtime import Runtime


class Layer:

    _diff = "diff"
    _meta = "meta"
    dirs = (_diff, _meta)

    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    @property
    def ref(self):
        return os.path.basename(self._root)

    @property
    def diffdir(self):

        return os.path.join(self._root, self._diff)

    @property
    def metadir(self):

        return os.path.join(self._root, self._meta)

    def read_metadata(self) -> tracking.Manifest:

        reader = tracking.MetadataReader(self.diffdir)
        return reader.read()

    def compute_metadata(self) -> tracking.Manifest:

        return tracking.compute_db(self.diffdir)


def _ensure_layer(path: str):

    os.makedirs(path, exist_ok=True)
    for subdir in Layer.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True)
    return Layer(path)


class LayerStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def read_layer(self, ref: str) -> Layer:

        layer_path = os.path.join(self._root, ref)
        if not os.path.exists(layer_path):
            raise ValueError(f"Unknown layer: {ref}")
        return Layer(layer_path)

    def _ensure_layer(self, ref: str) -> Layer:

        layer_dir = os.path.join(self._root, ref)
        return _ensure_layer(layer_dir)

    def remove_layer(self, ref: str) -> None:

        dirname = os.path.join(self._root, ref)
        try:
            shutil.rmtree(dirname)
        except OSError as e:
            if e.errno == errno.ENOENT:
                return
            raise

    def list_layers(self) -> List[Layer]:

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Layer(os.path.join(self._root, d)) for d in dirs]

    def commit_runtime(self, runtime: Runtime) -> Layer:

        mount_path = runtime.get_mount_path()
        if not mount_path:
            env_root = runtime.get_env_root()
            if not env_root:
                raise ValueError("runtime has no mount or environment data to commit")
            return self._commit_dir_unsafe(env_root)

        mount = Mount(mount_path)
        with mount.deactivated():
            return self._commit_dir_unsafe(mount.upperdir)

    def _commit_dir_unsafe(self, dirname: str) -> Layer:

        tmp_layer = self._ensure_layer("work-" + uuid.uuid1().hex)
        os.rmdir(tmp_layer.diffdir)
        shutil.copytree(dirname, tmp_layer.diffdir, symlinks=True)

        db = tmp_layer.compute_metadata()
        tree = db.get_path(tmp_layer.diffdir)
        assert tree is not None, "Manifest must have entry for layer diffdir"

        writer = tracking.MetadataWriter(tmp_layer.metadir)
        writer.rewrite_db(db, prefix=tmp_layer.diffdir)

        new_root = os.path.join(self._root, tree.digest)
        try:
            os.rename(tmp_layer._root, new_root)
        except OSError as e:
            self.remove_layer(tmp_layer.ref)
            if e.errno in (errno.EEXIST, errno.ENOTEMPTY):
                pass
            else:
                raise
        return self.read_layer(tree.digest)
