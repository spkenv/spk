import py.path
import pytest

from . import storage

from ._resolve import resolve_overlayfs_options


def test_runtime_overlay_args_basic_syntax(tmpdir: py.path.local) -> None:

    runtime = storage.Runtime(tmpdir.strpath)
    args = resolve_overlayfs_options(runtime)
    parts = args.split(",")
    for part in parts:
        _, _ = part.split("=")
