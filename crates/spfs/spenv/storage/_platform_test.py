import pytest

from ._runtime import _ensure_runtime
from ._platform import Platform, PlatformStorage, UnknownPlatformError


def test_platform_properties(tmpdir):

    platform = Platform(tmpdir.strpath)
    assert platform.ref
    assert platform.configfile
    assert platform.rootdir


def test_platform_read_layers_nofile(tmpdir):

    platform = Platform(tmpdir.strpath)
    actual = platform.read_layers()
    assert actual == []


def test_platform_read_write_layers(tmpdir):

    expected = ["a", "b", "c"]
    platform = Platform(tmpdir.strpath)
    platform._write_layers(expected)
    actual = platform.read_layers()
    assert actual == expected


def test_commit_runtime(tmpdir, mkpkg):

    runtime = _ensure_runtime(tmpdir.join("runtime").strpath)
    storage = PlatformStorage(tmpdir.join("platforms").strpath)

    first = storage.commit_runtime(runtime)
    second = storage.commit_runtime(runtime)
    assert first.ref == second.ref

    for _ in range(10):
        runtime.append_package(mkpkg())

    first = storage.commit_runtime(runtime)
    second = storage.commit_runtime(runtime)
    assert first.ref == second.ref

    runtime.append_package(mkpkg())
    second = storage.commit_runtime(runtime)
    assert first.ref != second.ref


def test_storage_remove_platform(tmpdir):

    storage = PlatformStorage(tmpdir.strpath)

    with pytest.raises(UnknownPlatformError):
        storage.remove_platform("non-existant")

    platform = storage._commit_layers([])
    storage.remove_platform(platform.ref)


def test_storage_list_platforms(tmpdir):

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
