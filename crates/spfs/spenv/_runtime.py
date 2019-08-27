from typing import NamedTuple, List, Optional
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
    config = Config()
    return storage.Runtime(path, config.repository())


def run(*cmd) -> subprocess.Popen:

    config = Config()
    repo = config.repository()
    runtimes = config.runtimes()

    runtime = runtimes.create_runtime()

    env = os.environ.copy()
    env["SPENV_RUNTIME"] = runtime.rootdir

    cmd = ("spenv-mount", runtime.overlay_args) + cmd

    return subprocess.Popen(cmd, env=env)
