from typing import NamedTuple, List, Optional
import re
import json
import subprocess

from . import storage


class FuseError(RuntimeError):
    """Error presented by the FUSE system."""

    pattern = re.compile(r"^fuse: (.*)$", re.RegexFlag.MULTILINE)


class FusermountError(RuntimeError):
    """Error presented by the fusermount command."""

    pattern = re.compile(r"^fusermount: (.*)$", re.RegexFlag.MULTILINE)


class OverlayFSError(RuntimeError):
    """Error presented by the fuse-overlayfs system."""

    pattern = re.compile(r"^fuse-overlayfs: (.*)$", re.RegexFlag.MULTILINE)


def mount(ref: str, target: str) -> None:
    """Mount an fs layer from the configured repository.

    Args:
        ref (str): reference to the target runtime
        target (str): path to a directory over which to mount the fs
    """

    repo = storage.configured_repository()
    return mount_from(repo, ref, target)


def mount_from(repo: storage.Repository, ref: str, target: str) -> None:
    """Mount a file system layer from the given repository.

    Args:
        repo (spfs.storage.Repository): the repository containing the desired runtime
        ref (str): reference to the target runtime
        target (str): the path to an existing directory over which to mount the layer
    """

    runtime = repo.runtimes.read_runtime(ref)
    parent_ref = runtime.get_parent_ref()
    if parent_ref is not None:
        lowerdirs = _resolve_lowerdirs(repo, ref)
    else:
        lowerdirs = [runtime.lowerdir]

    _fuse_overlayfs(
        "-o",
        f"lowerdir={':'.join(lowerdirs)},upperdir={runtime.upperdir},workdir={runtime.workdir}",
        target,
    )


def _resolve_lowerdirs(repo: storage.Repository, ref: str) -> List[str]:

    target = repo.read_ref(ref)
    if isinstance(target, storage.Runtime):
        parent_ref = target.get_parent_ref()
        if parent_ref is None:
            return []
        return _resolve_lowerdirs(repo, parent_ref)

    if isinstance(target, storage.Layer):
        return [target.diffdir]

    raise NotImplementedError(f"Unhandled ref type: {target}")


def unmount(target: str) -> None:
    """Unmount the mounted layer from the given directory."""

    mount_info = _get_mount_info(target)
    assert mount_info is not None, f"not mounted: {target}"
    assert (
        mount_info.source == "fuse-overlayfs"
    ), f"not a spenv-mounted file system {target}"
    assert (
        mount_info.fstype == "fuse.fuse-overlayfs"
    ), f"not a spenv-mounted file system {target}"
    print(_fusermount3("-u", target))


class MountInfo(NamedTuple):

    target: str
    source: str
    fstype: str
    options: str


def _get_mount_info(target: str) -> Optional[MountInfo]:

    json_str = _findmnt("-M", target, "--json").strip()
    if not json_str:
        return None
    json_data = json.loads(json_str)
    return MountInfo(**json_data["filesystems"][0])  # FIXME: this is janky accessor


def _findmnt(*args) -> str:

    cmd = ("findmnt",) + args
    proc = subprocess.Popen(cmd, stderr=subprocess.PIPE, stdout=subprocess.PIPE)
    out, err = proc.communicate()
    if proc.returncode == 0:
        return out.decode("utf-8")

    stderr = err.decode("utf-8")
    raise RuntimeError(" ".join(cmd), stderr)


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
