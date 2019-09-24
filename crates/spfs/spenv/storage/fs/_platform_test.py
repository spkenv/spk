from typing import Callable
import io

import py.path
import pytest

from ._runtime import _ensure_runtime
from ._layer import Layer
from ._platform import Platform, PlatformStorage, UnknownPlatformError


def test_platform_read_write_layers(tmpdir: py.path.local) -> None:

    expected = ("a", "b", "c")
    platform = Platform(layers=expected)
    stream = io.StringIO()
    platform.dump_json(stream)
    stream.seek(0, io.SEEK_SET)
    actual = Platform.load_json(stream)
    assert actual == platform


def test_commit_runtime(tmpdir: py.path.local, mklayer: Callable[[], Layer]) -> None:

    runtime = _ensure_runtime(tmpdir.join("runtime").strpath)
    storage = PlatformStorage(tmpdir.join("platforms").strpath)

    first = storage.commit_runtime(runtime)
    second = storage.commit_runtime(runtime)
    assert first.digest == second.digest

    for _ in range(10):
        runtime.append_layer(mklayer())

    first = storage.commit_runtime(runtime)
    second = storage.commit_runtime(runtime)
    assert first.digest == second.digest

    runtime.append_layer(mklayer())
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
