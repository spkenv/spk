from typing import NamedTuple
import os
import errno
import urllib

from . import tracking, storage
from ._config import Config


class NoWorkspaceError(ValueError):
    def __init__(self, path: str):

        super(NoWorkspaceError, self).__init__(f"No workspace: {path}")


class Workspace:

    _dotspenv = ".spenv"
    _runtimesdir = "runtimes"
    _mountsdir = "mounts"
    _metadir = "meta"
    dirs = (_runtimesdir, _mountsdir, _metadir)

    def __init__(self, root: str):

        self._root = os.path.abspath(root)
        self.runtimes = storage.RuntimeStorage(self.runtimesdir)
        self.mounts = storage.MountStorage(self.mountsdir)
        self.config = Config()

    @property
    def rootdir(self) -> str:
        return self._root

    @property
    def dotspenvdir(self) -> str:
        return os.path.join(self._root, self._dotspenv)

    @property
    def runtimesdir(self) -> str:
        return os.path.join(self.dotspenvdir, self._runtimesdir)

    @property
    def metadir(self) -> str:
        return os.path.join(self.dotspenvdir, self._metadir)

    @property
    def mountsdir(self) -> str:
        return os.path.join(self.dotspenvdir, self._mountsdir)

    def checkout(self, tag_str: str) -> None:

        tag = tracking.Tag.parse(tag_str)
        repos = self.config.repository_storage()
        repo = repos.read_repository(tag.path)
        repo.add_worktree(self.metadir, tag.version)

    def status(self) -> None:

        self._sync_meta()

    def _sync_meta(self) -> None:

        db = tracking.compute_db(self._root)



def create_workspace(path: str) -> Workspace:

    dotspenv_dir = os.path.join(path, Workspace._dotspenv)
    try:
        os.mkdir(path)
    except OSError as e:
        if e.errno != errno.EEXIST:
            raise
    try:
        os.mkdir(dotspenv_dir)
    except OSError as e:
        if e.errno == errno.EEXIST:
            raise ValueError(f"Workspace exists: {dotspenv_dir}")
        raise

    for name in Workspace.dirs:
        os.mkdir(os.path.join(dotspenv_dir, name))

    return read_workspace(path)


def read_workspace(path: str) -> Workspace:

    spenv_dir = os.path.join(path, Workspace._dotspenv)
    if not os.path.isdir(spenv_dir):
        raise NoWorkspaceError(path)

    return Workspace(path)


def discover_workspace(path: str) -> Workspace:

    prev_candidate = ""
    current_candidate = os.path.abspath(path)
    while prev_candidate != current_candidate:

        try:
            return read_workspace(current_candidate)
        except NoWorkspaceError:
            pass
        prev_candidate = current_candidate
        current_candidate = os.path.dirname(current_candidate)

    raise NoWorkspaceError(path)
