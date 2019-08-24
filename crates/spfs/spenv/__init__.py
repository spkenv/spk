"""Runtime environment management."""
from typing import List, Optional
import os
import sys
import errno
import subprocess

from . import storage
from ._config import Config
from ._workspace import (
    create_workspace,
    discover_workspace,
    Workspace,
    NoWorkspaceError,
    read_workspace,
    MASTER,
)


def active_runtime() -> Optional[storage.Runtime]:

    path = os.getenv("SPENV_RUNTIME")
    if not path:
        return None
    return storage.Runtime(path)
