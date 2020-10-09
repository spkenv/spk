import os

import pytest
import py.path

from .. import encoding
from ._storage import Runtime, Config, Storage, _ensure_runtime


def test_runtime_repr(tmpdir: py.path.local) -> None:

    runtime = Runtime(tmpdir.strpath)
    assert repr(runtime)


def test_config_serialization() -> None:

    expected = Config(stack=(encoding.NULL_DIGEST, encoding.EMPTY_DIGEST))
    data = expected.dump_dict()
    actual = Config.load_dict(data)

    assert actual == expected


def test_runtime_properties(tmpdir: py.path.local) -> None:

    runtime = Runtime(tmpdir.strpath)
    assert tmpdir.bestrelpath(runtime.root) == "."  # type: ignore
    assert os.path.basename(runtime.config_file) == Runtime._config_file


def test_runtime_config_notnone(tmpdir: py.path.local) -> None:

    runtime = Runtime(tmpdir.strpath)
    assert runtime._config is None
    assert runtime._get_config() is not None
    assert py.path.local(runtime.config_file).exists()


def test_ensure_runtime(tmpdir: py.path.local) -> None:

    runtime = _ensure_runtime(tmpdir.join("root").strpath)
    assert py.path.local(runtime.root).exists()
    assert py.path.local(runtime.upper_dir).exists()

    _ensure_runtime(runtime.root)


def test_storage_create_runtime(tmpdir: py.path.local) -> None:

    storage = Storage(tmpdir.strpath)

    runtime = storage.create_runtime()
    assert runtime.ref
    assert py.path.local(runtime.root).isdir()

    with pytest.raises(ValueError):
        storage.create_runtime(runtime.ref)


def test_storage_remove_runtime(tmpdir: py.path.local) -> None:

    storage = Storage(tmpdir.strpath)

    with pytest.raises(ValueError):
        storage.remove_runtime("non-existant")

    runtime = storage.create_runtime()
    storage.remove_runtime(runtime.ref)


def test_storage_list_runtimes(tmpdir: py.path.local) -> None:

    storage = Storage(tmpdir.join("root").strpath)

    assert storage.list_runtimes() == []

    storage.create_runtime()
    assert len(storage.list_runtimes()) == 1

    storage.create_runtime()
    storage.create_runtime()
    storage.create_runtime()
    assert len(storage.list_runtimes()) == 4


def test_runtime_reset(tmpdir: py.path.local) -> None:

    storage = Storage(tmpdir.join("root").strpath)
    runtime = storage.create_runtime()
    upper_dir = tmpdir.join("upper")
    runtime.upper_dir = upper_dir.strpath

    upper_dir.join("file").ensure()
    upper_dir.join("dir/file").ensure()
    upper_dir.join("dir/dir/dir/file").ensure()
    upper_dir.join("dir/dir/dir/file2").ensure()
    upper_dir.join("dir/dir/dir1/file").ensure()
    upper_dir.join("dir/dir2/dir/file.other").ensure()

    runtime.reset("file.*")
    assert not upper_dir.join("dir/dir2/dir/file.other").exists()
    assert upper_dir.join("dir/dir/dir/file2").exists()

    runtime.reset("dir1/")
    assert upper_dir.join("dir/dir/dir").exists()
    assert upper_dir.join("dir/dir2").exists()

    runtime.reset("/file")
    assert upper_dir.join("dir/dir/dir/file").exists()
    assert not upper_dir.join("file").exists()

    runtime.reset()
    assert os.listdir(upper_dir.strpath) == []
