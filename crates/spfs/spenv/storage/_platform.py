from typing import Optional, List, Dict, NamedTuple, Sequence
import os
import json
import uuid
import errno
import shutil
import hashlib

from ._runtime import Runtime
from ._layer import Layer


class UnknownPlatformError(ValueError):
    def __init__(self, ref: str) -> None:
        super(UnknownPlatformError, self).__init__(f"Unknown platform: {ref}")


class Platform(Layer):
    """Platforms represent a predetermined collection of packages.

    Platforms capture an entire runtime set of packages as a single,
    identifiable layer which can be applies/installed to future runtimes.
    """

    _configfile = "config.json"  # TODO: is this really a config?

    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    @property
    def ref(self) -> str:
        return os.path.basename(self._root)

    @property
    def configfile(self) -> str:
        return os.path.join(self._root, self._configfile)

    @property
    def rootdir(self) -> str:
        return self._root

    def read_layers(self) -> List[str]:
        try:
            with open(self.configfile, "r", encoding="utf-8") as f:
                data = json.load(f)
        except OSError as e:
            if e.errno == errno.ENOENT:
                return []
            raise

        assert isinstance(data, list), "Invalid configuration data: " + self.configfile
        return data

    def _write_layers(self, layers: Sequence[str]) -> None:

        with open(self.configfile, "w+", encoding="utf-8") as f:
            json.dump(layers, f)


class PlatformStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def read_platform(self, ref: str) -> Platform:
        """Read a platform's information from this storage.

        Raises:
            UnknownPlatformError: If the platform does not exist.
        """

        platform_path = os.path.join(self._root, ref)
        if not os.path.exists(platform_path):
            raise UnknownPlatformError(ref)
        return Platform(platform_path)

    def remove_platform(self, ref: str) -> None:
        """Remove a platform from this storage.

        Raises:
            UnknownPlatformError: If the platform does not exist.
        """

        platform_path = os.path.join(self._root, ref)
        try:
            shutil.rmtree(platform_path)
        except OSError as e:
            if e.errno == errno.ENOENT:
                raise UnknownPlatformError(ref)
            raise

    def list_platforms(self) -> List[Platform]:
        """Return a list of the current stored platforms."""

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Platform(os.path.join(self._root, d)) for d in dirs]

    def commit_runtime(self, runtime: Runtime) -> Platform:
        """Commit the current layer stack of a runtime as a platform."""

        return self._commit_layers(runtime.config.layers)

    def _commit_layers(self, layers: Sequence[str]) -> Platform:

        hasher = hashlib.sha256()
        for layer in layers:
            hasher.update(layer.encode("ascii"))

        ref = hasher.hexdigest()
        platform_dir = os.path.join(self._root, ref)
        try:
            os.makedirs(platform_dir)
        except OSError as e:
            if e.errno != errno.EEXIST:
                raise

        platform = Platform(platform_dir)
        platform._write_layers(layers)
        return platform
