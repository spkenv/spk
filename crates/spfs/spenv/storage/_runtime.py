from typing import Optional, List, NamedTuple, Tuple, Sequence, IO, ContextManager
import os
import re
import json
import uuid
import errno
import shutil
import hashlib
import subprocess
import contextlib

from ._package import Package


class MountConfig(NamedTuple):

    lowerdirs: Tuple[str, ...]

    @staticmethod
    def load(stream: IO[str]) -> "MountConfig":

        json_data = json.load(stream)
        json_data["lowerdirs"] = tuple(json_data.get("lowerdirs", []))
        return MountConfig(**json_data)

    def dump(self, stream: IO[str]):
        json.dump(self._asdict(), stream)


class Runtime:

    _upper = "upper"
    _work = "work"
    _lower = "lower"
    dirs = (_upper, _work, _lower)

    def __init__(self, root: str):

        self._root: str = os.path.abspath(root)
        self._config: Optional[MountConfig] = None

    @property
    def ref(self) -> str:
        return os.path.basename(self._root)

    @property
    def rootdir(self) -> str:
        return self._root

    @property
    def lowerdir(self):
        return os.path.join(self._root, self._lower)

    @property
    def configfile(self):
        return os.path.join(self._root, "config.json")

    @property
    def upperdir(self):
        return os.path.join(self._root, self._upper)

    @property
    def workdir(self):
        return os.path.join(self._root, self._work)

    @property
    def config(self):

        if self._config is None:
            return self._read_config()
        return self._config

    @property
    def overlay_args(self) -> str:
        return f"lowerdir={':'.join(self.config.lowerdirs)},upperdir={self.upperdir},workdir={self.workdir}"

    def append_package(self, package: Package) -> None:

        self._config = MountConfig(self.config.lowerdirs + (package.diffdir,))
        self._write_config()

    def _write_config(self):

        with open(self.configfile, "w+", encoding="utf-8") as f:
            self.config.dump(f)

    def _read_config(self) -> MountConfig:

        try:
            with open(self.configfile, "r", encoding="utf-8") as f:
                self._config = MountConfig.load(f)
        except OSError as e:
            if e.errno == errno.ENOENT:
                self._config = MountConfig(lowerdirs=(self.lowerdir,))
                self._write_config()
            else:
                raise
        return self._config


def _ensure_runtime(path: str):

    os.makedirs(path, exist_ok=True, mode=0o777)
    for subdir in Runtime.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True, mode=0o777)
    return Runtime(path)


class RuntimeStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def remove_runtime(self, name: str) -> None:

        runtime = self.read_runtime(name)
        # TODO: clobber read-only files
        # ensure unmounted first? by removing upperdir?
        shutil.rmtree(runtime.rootdir)

    def read_runtime(self, name: str) -> Runtime:

        runtime_dir = os.path.join(self._root, name)
        if not os.path.isdir(runtime_dir):
            raise ValueError("Unknown runtime: " + name)

        return Runtime(runtime_dir)

    def create_runtime(self, name: str = None) -> Runtime:

        if name is None:
            name = hashlib.sha256(uuid.uuid1().bytes).hexdigest()

        runtime_dir = os.path.join(self._root, name)
        try:
            os.makedirs(runtime_dir)
        except OSError as e:
            if e.errno == errno.EEXIST:
                raise ValueError("Runtime exists: " + name)
            raise
        return _ensure_runtime(runtime_dir)

    def list_runtimes(self) -> List[Runtime]:

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Runtime(os.path.join(self._root, d)) for d in dirs]
