"""Local file system storage of runtimes."""
from typing import Optional, List, NamedTuple, Tuple, Sequence, IO, Dict
import os
import re
import json
import uuid
import errno
import shutil
import hashlib
import pathlib
import subprocess
import contextlib

import simplejson

from .. import encoding
from . import _startup_csh, _startup_sh, _csh_exp

STARTUP_FILES_LOCATION = "/spfs/etc/spfs/startup.d"


class Config(NamedTuple):
    """Stores the configuration of a single runtime."""

    stack: Tuple[encoding.Digest, ...]
    editable: bool = False

    def dump_dict(self) -> Dict:
        """Dump this runtime data into a dictionary of python basic types."""

        return {"stack": list(s.str() for s in self.stack), "editable": self.editable}

    @staticmethod
    def load_dict(data: Dict) -> "Config":
        """Load a runtime data from the given dictionary data."""

        return Config(
            stack=tuple(encoding.parse_digest(d) for d in data.get("stack", [])),
            editable=data.get("editable", False),
        )


class Runtime:
    """Represents an active spfs session.

    The runtime contains the working files for a spfs
    envrionment, the contained stack of read-only filesystem layers.
    """

    upper_dir = "/tmp/spfs-runtime/upper"

    _config_file = "config.json"
    _sh_startup_file = "startup.sh"
    _csh_startup_file = "startup.csh"
    _csh_expect_file = "_csh.exp"

    def __init__(self, root: str) -> None:
        """Create a runtime to represent the data under 'root'."""

        self.root = os.path.abspath(root)
        try:
            os.makedirs(self.root, mode=0o777)
        except FileExistsError:
            pass
        else:
            # force the permissions on newly created dir,
            # since the above command passes 777 to the os
            # but the os may still create based on the current mask
            os.chmod(self.root, 0o777)

        self.config_file = os.path.join(self.root, self._config_file)
        self.sh_startup_file = os.path.join(self.root, self._sh_startup_file)
        self.csh_startup_file = os.path.join(self.root, self._csh_startup_file)
        self.csh_expect_file = os.path.join(self.root, self._csh_expect_file)

        self._config: Optional[Config] = None

    def __repr__(self) -> str:
        return f"Runtime('{self.root}')"

    @property
    def ref(self) -> str:
        """Return the identifier for this runtime."""
        return os.path.basename(self.root)

    def set_editable(self, editable: bool) -> None:
        """Mark this runtime as editable or not.

        An editable runtime is mounted with working directories
        that allow changes to be made to the runtime filesystem and
        committed back as layers.
        """

        config = self._get_config()
        self._config = Config(stack=config.stack, editable=editable)
        self._write_config()

    def is_editable(self) -> bool:
        """Return true if this runtime is editable.

        An editable runtime is mounted with working directories
        that allow changes to be made to the runtime filesystem and
        committed back as layers.
        """
        return self._get_config().editable

    def reset_stack(self) -> None:
        """Reset the config for this runtime to its default state."""

        self._get_config()
        self._config = Config(stack=tuple(), editable=False)
        self._write_config()

    def reset(self, *paths: str) -> None:
        """Remove working changes from this runtime's upper dir.

        If no paths are specified, reset all changes.
        """
        if not paths:
            paths = ("*",)

        for root, dirs, files in os.walk(self.upper_dir):
            root_path = pathlib.PurePath(root)
            for name in files:
                fullpath = root_path.joinpath(name)
                relpath = fullpath.relative_to(self.upper_dir)
                runpath = pathlib.PurePosixPath("/").joinpath(relpath)
                for path in paths:
                    if runpath.match(path):
                        os.remove(fullpath)
            for name in dirs:
                fullpath = root_path.joinpath(name)
                relpath = fullpath.relative_to(self.upper_dir)
                runpath = pathlib.PurePosixPath("/").joinpath(relpath)
                for path in paths:
                    if runpath.match(path):
                        shutil.rmtree(fullpath)

    def is_dirty(self) -> bool:
        """Return true if the upper dir of this runtime has changes."""

        try:
            return bool(os.listdir(self.upper_dir))
        except FileNotFoundError:
            return False
        return False

    def delete(self) -> None:
        """Remove all data pertaining to this runtime."""

        shutil.rmtree(self.root)

    def get_stack(self) -> Tuple[encoding.Digest, ...]:
        """Return this runtime's current object stack."""
        return self._get_config().stack

    def push_digest(self, digest: encoding.Digest) -> None:
        """Push an object id onto this runtime's stack.

        This will update the configuration of the runtime,
        and change the overlayfs options, but not update
        any currently running environment automatically.

        Args:
            digest (str): The digest of the object to push
        """

        try:
            assert len(digest) == encoding.DIGEST_SIZE
        except (ValueError, AssertionError):
            raise ValueError("Invalid digest: " + digest.str())

        self._config = Config((digest,) + self.get_stack())
        self._write_config()

    def _get_config(self) -> Config:

        if self._config is None:
            return self._read_config()
        return self._config

    def _write_config(self) -> None:

        if self._config is None:
            self._config = Config(stack=tuple())

        with open(self.config_file, "w+", encoding="utf-8") as f:
            json.dump(self._config.dump_dict(), f)

    def _read_config(self) -> Config:

        try:
            with open(self.config_file, "r", encoding="utf-8") as f:
                data = json.load(f)
            self._config = Config.load_dict(data)
        except OSError as e:
            if e.errno == errno.ENOENT:
                self._config = Config(stack=tuple())
                self._write_config()
            else:
                raise
        return self._config


def _ensure_runtime(path: str) -> Runtime:

    os.makedirs(path, exist_ok=True, mode=0o777)
    runtime = Runtime(path)
    os.makedirs(runtime.upper_dir, exist_ok=True, mode=0o777)
    with open(runtime.sh_startup_file, "w+") as f:
        f.write(_startup_sh.source)
    with open(runtime.csh_startup_file, "w+") as f:
        f.write(_startup_csh.source)
    with open(runtime.csh_expect_file, "w+") as f:
        f.write(_csh_exp.source)
    return runtime


class Storage:
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
        runtime.delete()

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
