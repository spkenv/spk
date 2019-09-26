from typing import Callable
import io

import py.path
import pytest

from ... import tracking
from ._runtime import _ensure_runtime
from ._layer import Layer
from ._platform import Platform, PlatformStorage, UnknownPlatformError


def test_commit_runtime(tmpdir: py.path.local) -> None:

    runtime = _ensure_runtime(tmpdir.join("runtime").strpath)
    storage = PlatformStorage(tmpdir.join("platforms").strpath)

    first = storage.commit_runtime(runtime)
    second = storage.commit_runtime(runtime)
    assert first.digest == second.digest

    manifest = tracking.compute_manifest("./spenv")
    layer = Layer(manifest=manifest, environ=tuple())
    for _ in range(10):
        runtime.append_layer(layer)

    first = storage.commit_runtime(runtime)
    second = storage.commit_runtime(runtime)
    assert first.digest == second.digest

    runtime.append_layer(layer)
    second = storage.commit_runtime(runtime)
    assert first.digest != second.digest


def test_storage_remove_platform(tmpdir: py.path.local) -> None:

    storage = PlatformStorage(tmpdir.strpath)

    with pytest.raises(UnknownPlatformError):
        storage.remove_platform("non-existant")

    platform = storage._commit_layers([])
    storage.remove_platform(platform.digest)


def test_storage_list_platforms(tmpdir: py.path.local) -> None:

    storage = PlatformStorage(tmpdir.join("root").strpath)

    assert storage.list_platforms() == []

    storage._commit_layers([])
    assert len(storage.list_platforms()) == 1

    storage._commit_layers([])
    assert len(storage.list_platforms()) == 1

    storage._commit_layers(["1"])
    storage._commit_layers(["1", "2"])
    storage._commit_layers(["1", "2", "3"])
    assert len(storage.list_platforms()) == 4
