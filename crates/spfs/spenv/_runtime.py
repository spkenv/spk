from typing import NamedTuple, List, Optional, Sequence
import os
import subprocess

from ._config import Config
from . import storage


class NoRuntimeError(EnvironmentError):
    def __init__(self, details: str = None) -> None:
        msg = "No active runtime"
        if details:
            msg += f": {details}"
        super(NoRuntimeError, self).__init__(msg)


def active_runtime() -> storage.Runtime:

    path = os.getenv("SPENV_RUNTIME")
    if path is None:
        raise NoRuntimeError()
    return storage.Runtime(path)


def exec_in_new_runtime(command: str, *args: str) -> subprocess.Popen:

    config = Config()
    repo = config.repository()
    runtime = repo.runtimes.create_runtime()

    return exec_in_runtime(runtime, command, *args)


def exec_in_runtime(
    runtime: storage.Runtime, command: str, *args: str
) -> subprocess.Popen:

    overlay_args = _resolve_overlayfs_options(runtime)
    env = os.environ.copy()
    env["SPENV_RUNTIME"] = runtime.rootdir
    return _spenv_mount(overlay_args, command, *args, env=env)


def _resolve_overlayfs_options(runtime: storage.Runtime) -> str:

    config = Config()
    repo = config.repository()
    lowerdirs = [runtime.lowerdir]
    packages = _resolve_layers_to_packages(runtime.config.layers)

    return f"lowerdir={':'.join(lowerdirs)},upperdir={runtime.upperdir},workdir={runtime.workdir}"


def _resolve_layers_to_packages(layers: Sequence[str]) -> List[storage.Package]:

    config = Config()
    repo = config.repository()
    packages = []
    for ref in layers:

        entry = repo.read_ref(ref)
        if isinstance(entry, storage.Package):
            packages.append(entry)
        else:
            expanded = _resolve_layers_to_packages(entry.layers)
            packages.extend(expanded)
    return packages


def _spenv_mount(overlay_args, *command, **popen_args) -> subprocess.Popen:

    cmd = ("spenv-mount", overlay_args) + command
    return subprocess.Popen(cmd, **popen_args)
