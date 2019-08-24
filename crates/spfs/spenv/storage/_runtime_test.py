from ._runtime import Runtime


def test_runtime_getset_parent_ref(tmpdir) -> None:

    runtime = Runtime(tmpdir.strpath)
    assert runtime.get_parent_ref() is None
    runtime.set_parent_ref(None)
    assert runtime.get_parent_ref() is None
    runtime.set_parent_ref("my_parent_ref")
    assert runtime.get_parent_ref() == "my_parent_ref"
    runtime.set_parent_ref(None)
    assert runtime.get_parent_ref() is None


def test_runtime_getset_env_root(tmpdir) -> None:

    runtime = Runtime(tmpdir.strpath)
    assert runtime.get_env_root() is None
    runtime.set_env_root(None)
    assert runtime.get_env_root() is None
    runtime.set_env_root("my_env_root")
    assert runtime.get_env_root() == "my_env_root"
    runtime.set_env_root(None)
    assert runtime.get_env_root() is None
