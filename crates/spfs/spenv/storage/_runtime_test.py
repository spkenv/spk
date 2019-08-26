import os

import pytest
import py.path

from ._runtime import RuntimeStorage, Runtime, RuntimeConfig, _ensure_runtime


def test_runtime_repr(tmpdir):

    runtime = Runtime(tmpdir.strpath)
    pkg_repr = repr(runtime)
    result = eval(pkg_repr)
    assert isinstance(result, Runtime)
    assert result.rootdir == runtime.rootdir


def test_config_serialization(tmpdir: py.path.local) -> None:

    tmpfile = tmpdir.join("config.json")
    expected = RuntimeConfig(lowerdirs=("a", "b", "c"))
    with open(tmpfile.strpath, "w+") as handle:
        expected.dump(handle)
    with open(tmpfile.strpath, "r") as handle:
        actual = RuntimeConfig.load(handle)

    assert actual == expected


def test_runtime_properties(tmpdir: py.path.local):

    runtime = Runtime(tmpdir.strpath)
    assert tmpdir.bestrelpath(runtime.rootdir) == "."
    assert os.path.basename(runtime.upperdir) == Runtime._upperdir
    assert os.path.basename(runtime.workdir) == Runtime._workdir
    assert os.path.basename(runtime.lowerdir) == Runtime._lowerdir
    assert os.path.basename(runtime.configfile) == Runtime._configfile


def test_runtime_config_notnone(tmpdir: py.path.local) -> None:

    runtime = Runtime(tmpdir.strpath)
    assert runtime._config is None
    assert runtime.config is not None
    assert py.path.local(runtime.configfile).exists()


def test_runtime_overlay_args_basic_syntax(tmpdir) -> None:

    runtime = Runtime(tmpdir.strpath)
    args = runtime.overlay_args
    parts = args.split(",")
    for part in parts:
        _, _ = part.split("=")


def test_ensure_runtime(tmpdir):

    runtime = _ensure_runtime(tmpdir.join("root").strpath)
    assert py.path.local(runtime.rootdir).exists()
    assert py.path.local(runtime.upperdir).exists()

    _ensure_runtime(runtime.rootdir)


def test_storage_create_runtime(tmpdir):

    storage = RuntimeStorage(tmpdir.strpath)

    runtime = storage.create_runtime()
    assert runtime.ref
    assert py.path.local(runtime.rootdir).isdir()

    with pytest.raises(ValueError):
        storage.create_runtime(runtime.ref)


def test_storage_remove_runtime(tmpdir):

    storage = RuntimeStorage(tmpdir.strpath)

    with pytest.raises(ValueError):
        storage.remove_runtime("non-existant")

    runtime = storage.create_runtime()
    storage.remove_runtime(runtime.ref)


def test_storage_list_runtimes(tmpdir):

    storage = RuntimeStorage(tmpdir.join("root").strpath)

    assert storage.list_runtimes() == []

    storage.create_runtime()
    assert len(storage.list_runtimes()) == 1

    storage.create_runtime()
    storage.create_runtime()
    storage.create_runtime()
    assert len(storage.list_runtimes()) == 4
