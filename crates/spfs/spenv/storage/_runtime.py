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


class RuntimeConfig(NamedTuple):
    """Stores the configuration of a single runtime."""

    lowerdirs: Tuple[str, ...]

    def dump(self, stream: IO[str]):
        """Dump this config as json to the given stream."""
        json.dump(self._asdict(), stream)

    @staticmethod
    def load(stream: IO[str]) -> "RuntimeConfig":
        """Load a runtime config from the given json stream."""

        json_data = json.load(stream)
        json_data["lowerdirs"] = tuple(json_data.get("lowerdirs", []))
        return RuntimeConfig(**json_data)


class Runtime:
    """Represents an active spenv session.

    The runtime contains the working files for a spenv
    envrionment, specifically the work and upper directories
    that are used by the overlay filesystem mount. It also
    retains a list of packages and platforms that have been
    install to the runtime, as well as the resulting stack
    of read-only filesystem layers.
    """

    _upperdir = "upper"
    _workdir = "work"
    _lowerdir = "lower"
    _configfile = "config.json"
    dirs = (_upperdir, _workdir, _lowerdir)

    def __init__(self, root: str) -> None:
        """Create a runtime to represent the data under 'root'."""

        self._root: str = os.path.abspath(root)
        self._config: Optional[RuntimeConfig] = None

    def __repr__(self) -> str:
        return f"Runtime('{self._root}')"

    @property
    def ref(self) -> str:
        """Return the identifier for this runtime."""
        return os.path.basename(self._root)

    @property
    def rootdir(self) -> str:
        """Return the root directory of this runtime."""
        return self._root

    @property
    def lowerdir(self) -> str:
        """Return the overlay fs lowerdir of this runtime."""
        return os.path.join(self._root, self._lowerdir)

    @property
    def configfile(self) -> str:
        """Return the path to this runtimes config file."""
        return os.path.join(self._root, self._configfile)

    @property
    def upperdir(self) -> str:
        """Return the overlay fs upperdir of this runtime."""
        return os.path.join(self._root, self._upperdir)

    @property
    def workdir(self) -> str:
        """Return the overlay fs workdir of this runtime."""
        return os.path.join(self._root, self._workdir)

    @property
    def config(self) -> RuntimeConfig:
        """Return this runtime's configuration data."""

        if self._config is None:
            return self._read_config()
        return self._config

    @property
    def overlay_args(self) -> str:
        """Return the overlayfs option string needed to mount this runtime."""
        return f"lowerdir={':'.join(self.config.lowerdirs)},upperdir={self.upperdir},workdir={self.workdir}"

    def append_package(self, package: Package) -> None:
        """Append a package to this runtime's stack.

        This will update the configuration of the runtime,
        and change the overlayfs options, but not update
        any currently running environment automatically.

        Args:
            package (Package): The package to append to the stack
        """

        self._config = RuntimeConfig(self.config.lowerdirs + (package.diffdir,))
        self._write_config()

    def _write_config(self) -> None:

        with open(self.configfile, "w+", encoding="utf-8") as f:
            self.config.dump(f)

    def _read_config(self) -> RuntimeConfig:

        try:
            with open(self.configfile, "r", encoding="utf-8") as f:
                self._config = RuntimeConfig.load(f)
        except OSError as e:
            if e.errno == errno.ENOENT:
                self._config = RuntimeConfig(lowerdirs=(self.lowerdir,))
                self._write_config()
            else:
                raise
        return self._config


def _ensure_runtime(path: str) -> Runtime:

    os.makedirs(path, exist_ok=True, mode=0o777)
    for subdir in Runtime.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True, mode=0o777)
    return Runtime(path)


class RuntimeStorage:
    """Manages the on-disk storage of many runtimes."""

    def __init__(self, root: str) -> None:
        """Initialize a new storage inside the given root directory."""

        self._root = os.path.abspath(root)

    def remove_runtime(self, ref: str) -> None:
        """Remove a runtime forcefully.

        Raises:
            ValueError: If the runtime does not exist
        """

        runtime = self.read_runtime(ref)
        # TODO: clobber read-only files
        # ensure unmounted first? by removing upperdir?
        shutil.rmtree(runtime.rootdir)

    def read_runtime(self, ref: str) -> Runtime:
        """Access a runtime in this storage.

        Args:
            ref (str): The identifier for the runtime to read.

        Raises:
            ValueError: If the runtime does not exist.

        Returns:
            Runtime: The identified runtime.
        """

        runtime_dir = os.path.join(self._root, ref)
        if not os.path.isdir(runtime_dir):
            raise ValueError("Unknown runtime: " + ref)

        return Runtime(runtime_dir)

    def create_runtime(self, ref: str = None) -> Runtime:
        """Create a new runtime.

        Args:
            ref (Optional[str]): the name of the new runtime,
                defaults to a new generated id.

        Raises:
            ValueError: If a runtime with the given ref already exists

        Returns:
            Runtime: The newly created runtime
        """

        if ref is None:
            ref = hashlib.sha256(uuid.uuid1().bytes).hexdigest()

        runtime_dir = os.path.join(self._root, ref)
        try:
            os.makedirs(runtime_dir)
        except OSError as e:
            if e.errno == errno.EEXIST:
                raise ValueError("Runtime exists: " + ref)
            raise
        return _ensure_runtime(runtime_dir)

    def list_runtimes(self) -> List[Runtime]:
        """List all stored runtimes.

        Returns:
            List[Runtime]: The stored runtimes.
        """

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Runtime(os.path.join(self._root, d)) for d in dirs]
