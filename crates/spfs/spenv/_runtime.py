from typing import NamedTuple, List, Optional
import os
import subprocess

from ._config import Config
from . import storage


def active_runtime() -> storage.Runtime:

    path = os.getenv("SPENV_RUNTIME")
    if path is None:
        raise RuntimeError("No active runtime")
    return storage.Runtime(path)


def run(*cmd) -> subprocess.Popen:

    config = Config()
    repo = config.repository()

    runtime = repo.runtimes.create_runtime()

    env = os.environ.copy()
    env["SPENV_RUNTIME"] = runtime.rootdir

    cmd = ("spenv-mount", runtime.overlay_args) + cmd
    print(cmd)

    return subprocess.Popen(cmd, env=env)
