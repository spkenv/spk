from typing import NamedTuple, Optional
import os
import errno
import urllib

import git

from . import tracking, storage
from ._config import Config

MASTER = "MASTER"


class NoWorkspaceError(ValueError):
    def __init__(self, path: str):

        super(NoWorkspaceError, self).__init__(f"No workspace: {path}")


class Workspace:

    _dotspenv = ".spenv"
    _runtimesdir = "run"
    _mountsdir = "mnt"
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

    def read_meta_repo(self) -> Optional[git.Repo]:

        try:
            return git.Repo(self.metadir)
        except git.InvalidGitRepositoryError:
            return None

    def checkout(self, tag_str: str) -> None:

        tag = tracking.Tag.parse(tag_str)
        repos = self.config.repository_storage()
        repo = repos.read_repository(tag.path)
        # TODO: check for dirtiness
        import shutil

        shutil.rmtree(self.metadir)
        repo.add_worktree(self.metadir, tag.version)

    def setup_runtime(self) -> storage.Runtime:

        mount = self._configure_mount()
        mount.activate()
        try:
            runtime = self.runtimes.read_runtime(MASTER)
        except ValueError:
            runtime = self.runtimes.create_runtime(MASTER)

        # TODO: properly set parent for runtime
        runtime.set_env_root(mount.mergeddir)
        runtime.set_mount_path(mount.rootdir)
        return runtime

    def _configure_mount(self) -> storage.Mount:

        try:
            mount = self.mounts.read_mount(MASTER)
        except ValueError:
            mount = self.mounts.create_mount(MASTER)

        mount.reconfigure([])  # FIXME: actually get lowerdirs
        return mount

    def commit(self, message: str) -> storage.Layer:

        layers = self.config.layer_storage()
        runtime = self.runtimes.read_runtime(MASTER)
        layer = layers.commit_runtime(runtime)
        metarepo = self.read_meta_repo()
        if not metarepo:
            return layer

        db = layer.read_metadata()
        writer = tracking.MetadataWriter(metarepo.working_dir)
        writer.rewrite_db(db, prefix=layer.diffdir)

        metarepo.index.add(".")
        commit = metarepo.commit(message)
        metarepo.create_tag(layer.ref, commit.hexsha)

        raise NotImplementedError("empty and reconfigure mount")
        # TODO: empty mount upper directory
        # TODO: reconfigure mount and runtime

    def diff(self) -> git.Diff:

        runtime = self.setup_runtime()
        repo = self.read_meta_repo()
        if not repo:
            raise RuntimeError("Workspace has no active tracking")
        db = tracking.compute_db(runtime.get_env_root())
        writer = tracking.MetadataWriter(self.metadir)
        writer.rewrite_db(db, prefix=runtime.get_env_root())
        repo.index.reset()
        repo.git.add(".")
        return repo.head.commit.diff(create_patch=True)


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
