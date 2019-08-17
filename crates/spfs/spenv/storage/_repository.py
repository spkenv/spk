from typing import List, Union
import os
import uuid
import errno
import shutil
import tarfile
import hashlib

from ._runtime import RuntimeStorage, Runtime
from ._layer import LayerStorage, Layer

Ref = Union[Layer, Runtime]


class Repository:

    _refs = "refs"
    _packs = "packs"
    _layers = "layers"
    _runtimes = "runtimes"
    dirs = (_refs, _layers, _runtimes)

    def __init__(self, root: str):

        self._root = os.path.abspath(root)
        self.layers = LayerStorage(self._join_path(self._layers))
        self.runtimes = RuntimeStorage(self._join_path(self._runtimes))

    def _join_path(self, *parts: str) -> str:

        return os.path.join(self._root, *parts)

    def read_ref(self, ref: str) -> Ref:

        # TODO: refs directory

        try:
            return self.layers.read_layer(ref)
        except ValueError:
            pass

        try:
            return self.runtimes.read_runtime(ref)
        except ValueError:
            pass

    def commit(self, ref: str) -> Layer:

        runtime = self.read_ref(ref)
        if not isinstance(runtime, Runtime):
            raise ValueError(f"Not a runtime: {ref}")

        return self.layers.commit_runtime(runtime)

    #     working_file = f"{uuid.uuid1().hex}"
    #     working_file = self._join_path(self._packs, working_file)

    #     # FIXME: should be unmounted first to ensure workdir is clean?
    #     # TODO: remove files from upperdir?
    #     # TODO: change runtime parent and remount?

    #     working_file = tarfile.shutil.make_archive(
    #         working_file, "tar", runtime.upperdir, owner=0, group=0
    #     )

    #     with open(working_file, "br") as f:
    #         hashfunc = hashlib.sha256()
    #         for byte_block in iter(lambda: f.read(4096), b""):
    #             hashfunc.update(byte_block)
    #         digest = hashfunc.hexdigest()

    #     pack_file = self._join_path(self._packs, f"{digest}.tar.gz")
    #     os.rename(working_file, pack_file)
    #     return self.unpack(digest)

    # def unpack(self, digest: str) -> Layer:

    #     pack_filename = self._join_path(self._packs, f"{digest}.tar.gz")
    #     layer = self.layers.ensure_layer(digest)
    #     tarfile.shutil.unpack_archive(pack_filename, layer.diffdir, "gztar")
    #     return layer


def ensure_repository(path: str) -> Repository:

    os.makedirs(path, exist_ok=True)
    for subdir in Repository.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True)

    return Repository(path)
