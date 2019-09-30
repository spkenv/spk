import os

import pytest
import py.path

from ._runtime_storage import Runtime, RuntimeConfig, RuntimeStorage, _ensure_runtime


def test_runtime_repr(tmpdir: py.path.local) -> None:

    runtime = Runtime(tmpdir.strpath)
    assert repr(runtime)


def test_config_serialization() -> None:

    expected = RuntimeConfig(stack=("a", "b", "c"))
    data = expected.dump_dict()
    actual = RuntimeConfig.load_dict(data)

    assert actual == expected


def test_runtime_properties(tmpdir: py.path.local) -> None:

    runtime = Runtime(tmpdir.strpath)
    assert tmpdir.bestrelpath(runtime.root) == "."
    assert os.path.basename(runtime.upper_dir) == Runtime._upperdir
    assert os.path.basename(runtime.work_dir) == Runtime._workdir
    assert os.path.basename(runtime.lower_dir) == Runtime._lowerdir
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

    storage = RuntimeStorage(tmpdir.strpath)

    runtime = storage.create_runtime()
    assert runtime.ref
    assert py.path.local(runtime.root).isdir()

    with pytest.raises(ValueError):
        storage.create_runtime(runtime.ref)


def test_storage_remove_runtime(tmpdir: py.path.local) -> None:

    storage = RuntimeStorage(tmpdir.strpath)

    with pytest.raises(ValueError):
        storage.remove_runtime("non-existant")

    runtime = storage.create_runtime()
    storage.remove_runtime(runtime.ref)


def test_storage_list_runtimes(tmpdir: py.path.local) -> None:

    storage = RuntimeStorage(tmpdir.join("root").strpath)

    assert storage.list_runtimes() == []

    storage.create_runtime()
    assert len(storage.list_runtimes()) == 1

    storage.create_runtime()
    storage.create_runtime()
    storage.create_runtime()
    assert len(storage.list_runtimes()) == 4
