from typing import Tuple, Dict, Optional, List
import os
import errno
import subprocess

import structlog

from ._config import get_config
from ._runtime import active_runtime, compute_runtime_manifest
from ._resolve import resolve_overlay_dirs, which
from . import storage, runtime, tracking

_logger = structlog.get_logger(__name__)


def build_command_for_runtime(
    runtime: runtime.Runtime, command: str, *args: str
) -> Tuple[str, ...]:
    """Construct a bootstrap command.

    The returned command properly calls through the relevant spfs
    binaries and runs the desired command in an existing runtime.
    """

    spfs_exe = which("spfs")
    if not spfs_exe:
        raise RuntimeError("'spfs' not found in PATH")

    args = ("init-runtime", runtime.root, command) + args

    return _build_spfs_enter_command(runtime, spfs_exe, *args)


def build_interactive_shell_cmd() -> Tuple[str, ...]:
    """Return a command that initializes and runs an interactive shell

    The returned command properly sets up and runs an interactive
    shell session in the current runtime.
    """

    rt = active_runtime()
    shell_path = os.environ.get("SHELL", "<not-set>")
    shell_name = os.path.basename(shell_path)

    if shell_name in ("tcsh",):
        expect = which("expect")
        if expect is None:
            _logger.error("'expect' command not found in PATH, falling back to bash")
            shell_name = "bash"
        else:
            return (expect, rt.csh_expect_file, shell_path, rt.csh_startup_file)

    if shell_name not in ("bash",):
        _logger.warning(f"current shell not supported ({shell_path}) - using bash")
        shell_path = "/usr/bin/bash"
        shell_name = "bash"
    return (shell_path, "--init-file", rt.sh_startup_file)


def build_shell_initialized_command(command: str, *args: str) -> Tuple[str, ...]:
    """Construct a boostrapping command for initializing through the shell.

    The returned command properly calls through a shell which sets up
    the current runtime appropriately before calling the desired command.
    """

    runtime = active_runtime()
    default_shell = which("bash") or ""
    desired_shell = os.environ.get("SHELL", default_shell)
    shell_name = os.path.basename(desired_shell)
    if shell_name in ("bash", "sh"):
        startup_file = runtime.sh_startup_file
    elif shell_name in ("tcsh", "csh"):
        startup_file = runtime.csh_startup_file
    else:
        raise RuntimeError("No supported shell found, or no support for current shell")

    return (desired_shell, startup_file, command) + args


def _build_spfs_enter_command(rt: runtime.Runtime, *command: str) -> Tuple[str, ...]:

    exe = which("spfs-enter")
    if exe is None:
        raise RuntimeError("'spfs-enter' not found in PATH")

    args = [exe]

    overlay_dirs = resolve_overlay_dirs(rt)
    for dirpath in overlay_dirs:
        args.extend(["-d", dirpath])

    if rt.is_editable():
        args.append("-e")

    _logger.debug("computing runtime manifest")
    manifest = compute_runtime_manifest(rt)

    _logger.debug("finding files that should be masked")
    for path, entry in manifest.walk_abs("/spfs"):
        if entry.kind != tracking.EntryKind.MASK:
            continue
        args.extend(("-m", path))

    args.append("--")

    return tuple(args) + command
