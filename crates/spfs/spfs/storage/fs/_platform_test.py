from typing import Callable
import io

import py.path
import pytest

from ... import tracking
from ._layer import Layer
from ._platform import Platform, PlatformStorage


def test_commit_stack(tmpdir: py.path.local) -> None:

    storage = PlatformStorage(tmpdir.join("platforms").strpath)

    first = storage.commit_stack([])
    second = storage.commit_stack([])
    assert first.digest == second.digest

    manifest = tracking.compute_manifest("./spfs")
    stack = ["my_layer" for _ in range(10)]

    first = storage.commit_stack(stack)
    second = storage.commit_stack(stack)
    assert first.digest == second.digest

    stack.append("another_entry")
    second = storage.commit_stack(stack)
    assert first.digest != second.digest


def test_storage_remove_platform(tmpdir: py.path.local) -> None:

    storage = PlatformStorage(tmpdir.strpath)

    with pytest.raises(ValueError):
        storage.remove_platform("non-existant")

    platform = storage.commit_stack([])
    storage.remove_platform(platform.digest)


def test_storage_list_platforms(tmpdir: py.path.local) -> None:

    storage = PlatformStorage(tmpdir.join("root").strpath)

    assert storage.list_platforms() == []

    storage.commit_stack([])
    assert len(storage.list_platforms()) == 1

    storage.commit_stack([])
    assert len(storage.list_platforms()) == 1

    storage.commit_stack(["1"])
    storage.commit_stack(["1", "2"])
    storage.commit_stack(["1", "2", "3"])
    assert len(storage.list_platforms()) == 4
