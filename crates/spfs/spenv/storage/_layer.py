from typing import NamedTuple, Tuple, List
import os
import enum
import uuid
import stat
import errno
import shutil
import hashlib

from ._runtime import Runtime


class Layer:

    _diff = "diff"
    dirs = (_diff,)

    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    @property
    def ref(self):
        return os.path.basename(self._root)

    @property
    def diffdir(self):

        return os.path.join(self._root, self._diff)

    def compute_digest(self):

        return tracking.compute_tree(self.diffdir).digest


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

    def ensure_layer(self, ref: str) -> Layer:

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

        tmp_layer = self.ensure_layer(uuid.uuid1().hex)
        # FIXME: does this need to be unmounted from everywhere?
        # FIXME: does the runtime need to have a new parent and remount?
        os.rename(runtime.upperdir, tmp_layer.diffdir)
        os.mkdir(runtime.upperdir)
        digest = tmp_layer.compute_digest()
        new_root = os.path.join(self._root, digest)
        try:
            os.rename(tmp_layer._root, new_root)
        except OSError as e:
            if e.errno in (errno.EEXIST, errno.ENOTEMPTY):
                self.remove_layer(tmp_layer.ref)
            else:
                raise
        return self.read_layer(digest)
