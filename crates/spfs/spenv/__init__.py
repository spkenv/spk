"""Runtime environment management."""
from typing import List, Optional
import os
import sys
import errno
import subprocess

from . import storage, tracking, runtime, io
from ._config import get_config, load_config, Config
from ._runtime import (
    active_runtime,
    initialize_runtime,
    deinitialize_runtime,
    NoRuntimeError,
)
from ._bootstrap import (
    build_command_for_runtime,
    build_shell_initialized_command,
    build_interactive_shell_command,
)
from ._sync import push_ref, pull_ref
from ._commit import commit_layer, commit_platform

__version__ = "0.10.3"
