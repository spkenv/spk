from typing import NamedTuple, Tuple, List
import os
import enum
import uuid
import stat
import errno
import shutil
import hashlib

from .. import tracking
from ._layer import Layer


class Package(Layer):

    _diff = "diff"
    _meta = "meta"
    dirs = (_diff, _meta)

    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    def __repr__(self):

        return f"Package('{self.rootdir}')"

    @property
    def ref(self):
        return os.path.basename(self._root)

    @property
    def rootdir(self):
        return self._root

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

        return tracking.compute_manifest(self.diffdir)


def _ensure_package(path: str):

    os.makedirs(path, exist_ok=True, mode=0o777)
    for subdir in Package.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True, mode=0o777)
    return Package(path)


class PackageStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def read_package(self, ref: str) -> Package:

        package_path = os.path.join(self._root, ref)
        if not os.path.exists(package_path):
            raise ValueError(f"Unknown package: {ref}")
        return Package(package_path)

    def _ensure_package(self, ref: str) -> Package:

        package_dir = os.path.join(self._root, ref)
        return _ensure_package(package_dir)

    def remove_package(self, ref: str) -> None:

        dirname = os.path.join(self._root, ref)
        try:
            shutil.rmtree(dirname)
        except OSError as e:
            if e.errno == errno.ENOENT:
                return
            raise

    def list_packages(self) -> List[Package]:

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Package(os.path.join(self._root, d)) for d in dirs]

    def commit_dir(self, dirname: str) -> Package:

        tmp_package = self._ensure_package("work-" + uuid.uuid1().hex)
        os.rmdir(tmp_package.diffdir)
        shutil.copytree(dirname, tmp_package.diffdir, symlinks=True)

        db = tmp_package.compute_metadata()
        tree = db.get_path(tmp_package.diffdir)
        assert tree is not None, "Manifest must have entry for package diffdir"

        writer = tracking.MetadataWriter(tmp_package.metadir)
        writer.rewrite_db(db, prefix=tmp_package.diffdir)

        new_root = os.path.join(self._root, tree.digest)
        try:
            os.rename(tmp_package._root, new_root)
        except OSError as e:
            self.remove_package(tmp_package.ref)
            if e.errno in (errno.EEXIST, errno.ENOTEMPTY):
                pass
            else:
                raise
        return self.read_package(tree.digest)
