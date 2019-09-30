"""Local file system storage of runtimes."""
from typing import Optional, List, NamedTuple, Tuple, Sequence, IO, Dict
import os
import re
import json
import uuid
import errno
import shutil
import hashlib
import subprocess
import contextlib

import simplejson

from . import storage, tracking

STARTUP_FILES_LOCATION = "/env/etc/spenv/startup.d"
_SH_STARTUP_SCRIPT = """
#!/usr/bin/env sh
startup_dir="/env/etc/spenv/startup.d"
if [[ -d ${startup_dir} ]]; then
    for file in $(ls ${startup_dir}); do
        echo source ${startup_dir}/$file
        source ${startup_dir}/$file
    done
fi
"$*"
exit $?
"""


class RuntimeConfig(NamedTuple):
    """Stores the configuration of a single runtime."""

    stack: Tuple[str, ...]

    def dump_dict(self) -> Dict:
        """Dump this runtime data into a dictionary of python basic types."""

        return {"stack": list(self.stack)}

    @staticmethod
    def load_dict(data: Dict) -> "RuntimeConfig":
        """Load a runtime data from the given dictionary data."""

        return RuntimeConfig(stack=tuple(data.get("stack", [])))


class Runtime:
    """Represents an active spenv session.

    The runtime contains the working files for a spenv
    envrionment, specifically the work and upper directories
    that are used by the overlay filesystem mount. It also
    retains a list of layers and platforms that have been
    install to the runtime, as well as the resulting stack
    of read-only filesystem layers.
    """

    _upperdir = "upper"
    _workdir = "work"
    _lowerdir = "lower"
    _config_file = "config.json"
    _sh_startup_file = "startup.sh"
    dirs = (_upperdir, _workdir, _lowerdir)

    def __init__(self, root: str) -> None:
        """Create a runtime to represent the data under 'root'."""

        self.root = os.path.abspath(root)
        self.lower_dir = os.path.join(self.root, self._lowerdir)
        self.config_file = os.path.join(self.root, self._config_file)
        self.sh_startup_file = os.path.join(self.root, self._sh_startup_file)
        self.upper_dir = os.path.join(self.root, self._upperdir)
        self.work_dir = os.path.join(self.root, self._workdir)

        self._config: Optional[RuntimeConfig] = None

    def __repr__(self) -> str:
        return f"Runtime('{self.root}')"

    @property
    def ref(self) -> str:
        """Return the identifier for this runtime."""
        return os.path.basename(self.root)

    def get_stack(self) -> Tuple[str, ...]:
        """Return this runtime's current object stack."""
        return self._get_config().stack

    def push_digest(self, digest: str) -> None:
        """Push an object id onto this runtime's stack.

        This will update the configuration of the runtime,
        and change the overlayfs options, but not update
        any currently running environment automatically.

        Args:
            digest (str): The digest of the object to push
        """

        try:
            digest_bytes = bytearray.fromhex(digest)
            assert len(digest_bytes) == hashlib.sha256().digest_size
        except (ValueError, AssertionError):
            raise ValueError("Invalid digest: " + digest)

        self._config = RuntimeConfig(self.get_stack() + (digest,))
        self._write_config()

    def _get_config(self) -> RuntimeConfig:

        if self._config is None:
            return self._read_config()
        return self._config

    def _write_config(self) -> None:

        if self._config is None:
            self._config = RuntimeConfig(stack=tuple())

        with open(self.config_file, "w+", encoding="utf-8") as f:
            json.dump(self._config.dump_dict(), f)

    def _read_config(self) -> RuntimeConfig:

        try:
            with open(self.config_file, "r", encoding="utf-8") as f:
                data = json.load(f)
            self._config = RuntimeConfig.load_dict(data)
        except OSError as e:
            if e.errno == errno.ENOENT:
                self._config = RuntimeConfig(stack=tuple())
                self._write_config()
            else:
                raise
        return self._config


def _ensure_runtime(path: str) -> Runtime:

    os.makedirs(path, exist_ok=True, mode=0o777)
    for subdir in Runtime.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True, mode=0o777)
    runtime = Runtime(path)
    with open(runtime.sh_startup_file, "w+", encoding="utf-8") as f:
        f.write(_SH_STARTUP_SCRIPT)
    return runtime


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
        shutil.rmtree(runtime.root)

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
