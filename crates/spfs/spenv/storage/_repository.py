from typing import List, Union
import os
import uuid
import errno
import shutil
import tarfile
import hashlib

from ._platform import PlatformStorage, Platform
from ._package import PackageStorage, Package
from ._runtime import RuntimeStorage, Runtime
from ._layer import Layer


class Repository:

    _pack = "pack"
    _plat = "plat"
    _tag = "tags"
    _run = "run"
    dirs = (_pack, _plat, _tag, _run)

    def __init__(self, root: str):

        self._root = os.path.abspath(root)
        self.packages = PackageStorage(self._join_path(self._pack))
        self.platforms = PlatformStorage(self._join_path(self._plat))
        self.runtimes = RuntimeStorage(self._join_path(self._run))

    def _join_path(self, *parts: str) -> str:

        return os.path.join(self._root, *parts)

    def read_ref(self, ref: str) -> Layer:

        tag_path = self._join_path(self._tag, ref)
        try:
            target = os.readlink(tag_path)
            # TODO: this feels janky
            return self.read_ref(os.path.basename(target))
        except OSError as e:
            if e.errno == errno.ENOENT:
                pass
            else:
                raise

        try:
            return self.packages.read_package(ref)
        except ValueError:
            pass

        try:
            return self.platforms.read_platform(ref)
        except ValueError:
            pass

        raise ValueError("Unknown ref: " + ref)

    def commit_package(self, runtime: Runtime) -> Package:
        """Commit the working file changes of a runtime to a new package."""

        return self.packages.commit_dir(runtime.upperdir)

    def tag(self, ref: str, tag: str) -> None:

        layer = self.read_ref(ref)
        tagdir = self._join_path(self._tag)
        linkfile = os.path.join(tagdir, tag)
        os.makedirs(os.path.basename(linkfile), exist_ok=True)
        os.symlink(layer.rootdir, linkfile)
        # TODO: test overwriting


def ensure_repository(path: str) -> Repository:

    os.makedirs(path, exist_ok=True, mode=0o777)
    for subdir in Repository.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True, mode=0o777)

    return Repository(path)
