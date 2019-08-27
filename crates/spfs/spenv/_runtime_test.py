import pytest

from . import storage

from ._runtime import _resolve_overlayfs_options


def test_runtime_overlay_args_basic_syntax(tmpdir) -> None:

    runtime = storage.Runtime(tmpdir.strpath)
    args = _resolve_overlayfs_options(runtime)
    parts = args.split(",")
    for part in parts:
        _, _ = part.split("=")
