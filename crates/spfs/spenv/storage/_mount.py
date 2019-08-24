from typing import Optional, List, NamedTuple, Tuple, Sequence, IO, ContextManager
import os
import re
import json
import errno
import subprocess
import contextlib


class MountConfig(NamedTuple):

    lowerdirs: Tuple[str, ...]

    @staticmethod
    def load(stream: IO[str]) -> "MountConfig":

        json_data = json.load(stream)
        json_data["lowerdirs"] = tuple(json_data.get("lowerdirs", []))
        return MountConfig(**json_data)

    def dump(self, stream: IO[str]):
        json.dump(self._asdict(), stream)


class Mount:

    _upper = "upper"
    _work = "work"
    _lower = "lower"
    _merged = "merged"
    dirs = (_upper, _work, _lower, _merged)

    def __init__(self, root: str):

        self._root = os.path.abspath(root)

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
    def mergeddir(self):
        return os.path.join(self._root, self._merged)

    def reconfigure(self, lowerdirs: Sequence[str]) -> None:

        abs_dirs: Tuple[str, ...] = (self.lowerdir,)
        for dirname in lowerdirs:
            abs_dirs += (os.path.abspath(dirname),)
        config = MountConfig(lowerdirs=abs_dirs)
        with open(self.configfile, "w+", encoding="utf-8") as f:
            config.dump(f)
        if self.is_active():
            self.deactivate()
            self.activate()

    def read_config(self) -> MountConfig:

        with open(self.configfile, "r", encoding="utf-8") as f:
            return MountConfig.load(f)

    def is_active(self) -> bool:

        return _get_mount_info(self.mergeddir) is not None

    def activate(self):

        if self.is_active():
            return

        config = self.read_config()
        _fuse_overlayfs(
            "-o",
            f"lowerdir={':'.join(config.lowerdirs)},upperdir={self.upperdir},workdir={self.workdir}",
            self.mergeddir,
        )

    def deactivate(self):

        if not self.is_active():
            return

        _fusermount3("-u", self.mergeddir)

    def deactivated(self) -> ContextManager:

        return deactivated_mount(self)


@contextlib.contextmanager
def deactivated_mount(mount):

    should_reactivate = mount.is_active()
    mount.deactivate()
    try:
        yield mount
    finally:
        if should_reactivate:
            mount.activate()


def _ensure_mount(path: str):

    os.makedirs(path, exist_ok=True)
    for subdir in Mount.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True)
    return Mount(path)


class MountStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def read_mount(self, name: str) -> Mount:

        mount_dir = os.path.join(self._root, name)
        if not os.path.isdir(mount_dir):
            raise ValueError("Unknown mount: " + name)

        return Mount(mount_dir)

    def create_mount(self, name: str, lowerdirs: Sequence[str] = None) -> Mount:

        mount_dir = os.path.join(self._root, name)
        mount = _ensure_mount(mount_dir)
        if lowerdirs is None:
            lowerdirs = [mount.lowerdir]

        mount.reconfigure(lowerdirs)
        return mount


class FuseError(RuntimeError):
    """Error presented by the FUSE system."""

    pattern = re.compile(r"^fuse: (.*)$", re.RegexFlag.MULTILINE)


class FusermountError(RuntimeError):
    """Error presented by the fusermount command."""

    pattern = re.compile(r"^fusermount: (.*)$", re.RegexFlag.MULTILINE)


class OverlayFSError(RuntimeError):
    """Error presented by the fuse-overlayfs system."""

    pattern = re.compile(r"^fuse-overlayfs: (.*)$", re.RegexFlag.MULTILINE)


def _fusermount3(*args) -> str:

    return _exec_fuse_command("fusermount3", *args)


def _fuse_overlayfs(*args) -> None:
    """Run the fuse-overlayfs command with the given arguments.

    Raises:
        FuseError: when a FUSE error is detected after command failure
        OverlayFSError: when an overlayfs error is detected after command failure
        RuntimeError: when the command fails and no specific error is detected

    Returns:
        str: The standard output of the executed command
    """

    _exec_fuse_command("fuse-overlayfs", *args)


def _exec_fuse_command(*cmd) -> str:

    proc = subprocess.Popen(cmd, stderr=subprocess.PIPE, stdout=subprocess.PIPE)
    out, err = proc.communicate()
    if proc.returncode == 0:
        return out.decode("utf-8")

    stderr = err.decode("utf-8")

    fuse_error = FuseError.pattern.match(stderr)
    if fuse_error:
        raise FuseError(fuse_error.group(1))

    fusermount_err = FusermountError.pattern.match(stderr)
    if fuse_error:
        raise FusermountError(fuse_error.group(1))

    overlay_error = OverlayFSError.pattern.match(stderr)
    if overlay_error:
        raise OverlayFSError(overlay_error.group(1))

    stdout = out.decode("utf-8")
    raise RuntimeError(
        f"fuse-overlayfs exit status: {proc.returncode}\nstdout: {stdout}\nstderr: {stderr}"
    )


class _MountInfo(NamedTuple):

    target: str
    source: str
    fstype: str
    options: str


def _get_mount_info(target: str) -> Optional[_MountInfo]:

    json_str = _findmnt_json("-M", target)
    json_data = json.loads(json_str)
    filesystems = json_data.get("filesystems", [])
    for filesystem in filesystems:
        if filesystem.get("target") == target:
            return _MountInfo(**filesystem)
    else:
        return None


def _findmnt_json(*args) -> str:

    cmd = ("findmnt", "--json") + args
    proc = subprocess.Popen(cmd, stderr=subprocess.PIPE, stdout=subprocess.PIPE)
    out, _ = proc.communicate()
    if proc.returncode == 0:
        return out.decode("utf-8").strip()
    return "{}"
