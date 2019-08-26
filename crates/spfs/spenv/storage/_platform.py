from typing import Optional, List, Dict
import os
import uuid
import errno
import shutil
import hashlib


from ._layer import Layer


class Platform(Layer):
    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    def __repr__(self):
        return f"Platform('{self.rootdir}')"

    @property
    def rootdir(self) -> str:
        return self._root


def _ensure_platform(path: str) -> Platform:

    os.makedirs(path, exist_ok=True, mode=0o777)
    return Platform(path)


class PlatformStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def read_platform(self, ref: str) -> Platform:

        platform_path = os.path.join(self._root, ref)
        if not os.path.exists(platform_path):
            raise ValueError(f"Unknown platform: {ref}")
        return Platform(platform_path)

    def remove_platform(self, ref: str) -> None:

        platform_path = os.path.join(self._root, ref)
        shutil.rmtree(platform_path)
        # FIXME: does this error when not exist?

    def list_platforms(self) -> List[Platform]:

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Platform(os.path.join(self._root, d)) for d in dirs]

    def create_platform(self, name: str) -> Platform:

        platform_dir = os.path.join(self._root, name)
        try:
            os.makedirs(platform_dir)
        except OSError as e:
            if e.errno == errno.EEXIST:
                raise ValueError("Platform exists: " + name)
            raise
        return _ensure_platform(platform_dir)
