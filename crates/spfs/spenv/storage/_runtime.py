from typing import Optional, List, Dict
import os
import uuid
import errno
import shutil
import hashlib

from ._variables import Variables


class Runtime:
    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    @property
    def rootdir(self) -> str:
        return self._root

    @property
    def parent_file(self):
        return os.path.join(self._root, "parent")

    @property
    def env_root_file(self):
        return os.path.join(self._root, "env_root")

    @property
    def mount_file(self):
        return os.path.join(self._root, "mount")

    def set_mount_path(self, path: Optional[str]) -> None:
        _write_data_file(self.mount_file, path)

    def get_mount_path(self) -> Optional[str]:
        return _read_data_file(self.mount_file)

    def set_parent_ref(self, ref: Optional[str]) -> None:
        _write_data_file(self.parent_file, ref)

    def get_parent_ref(self) -> Optional[str]:
        return _read_data_file(self.parent_file)

    def set_env_root(self, rootdir: Optional[str]) -> None:
        _write_data_file(self.env_root_file, rootdir)

    def get_env_root(self) -> Optional[str]:
        return _read_data_file(self.env_root_file)

    def compile_environment(self, base: Dict[str, str] = None) -> Dict[str, str]:

        if base is None:
            base = os.environ

        # TODO: calculate environment from parent

        env: Dict[str, str] = base.copy()
        env["SPENV_PARENT"] = self.get_parent_ref() or ""
        env["SPENV_RUNTIME"] = self._root
        env["SPENV_ROOT"] = self.get_env_root() or ""
        return env


def _ensure_runtime(path: str):

    os.makedirs(path, exist_ok=True)
    return Runtime(path)


class RuntimeStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def read_runtime(self, ref: str) -> Runtime:

        runtime_path = os.path.join(self._root, ref)
        if not os.path.exists(runtime_path):
            raise ValueError(f"Unknown runtime: {ref}")
        return Runtime(runtime_path)

    def remove_runtime(self, ref: str) -> None:

        runtime_path = os.path.join(self._root, ref)
        shutil.rmtree(runtime_path)
        # FIXME: does this error when not exist?

    def list_runtimes(self) -> List[Runtime]:

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Runtime(os.path.join(self._root, d)) for d in dirs]

    def create_runtime(self, name: str) -> Runtime:

        runtime_dir = os.path.join(self._root, name)
        try:
            os.makedirs(runtime_dir)
        except OSError as e:
            if e.errno == errno.EEXIST:
                raise ValueError("Runtime exists: " + name)
            raise
        return _ensure_runtime(runtime_dir)


def _write_data_file(filepath: str, value: Optional[str]) -> None:

    if value is None:
        try:
            return os.remove(filepath)
        except OSError as e:
            if e.errno == errno.ENOENT:
                return
            raise

    with open(filepath, "w+", encoding="utf-8") as f:
        f.write(value)


def _read_data_file(filepath: str) -> Optional[str]:

    try:
        with open(filepath, encoding="utf-8") as f:
            return f.read()
    except OSError as e:
        if e.errno == errno.ENOENT:
            return None
        raise
