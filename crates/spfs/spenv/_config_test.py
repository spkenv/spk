import pytest
import py.path

from . import storage
from ._config import Config, get_config, load_config


def test_config_list_remote_names_empty() -> None:

    config = Config()
    assert config.list_remote_names() == []


def test_config_list_remote_names() -> None:

    config = Config()
    config.read_string("[remote.origin]\naddress=http://myaddres")
    assert config.list_remote_names() == ["origin"]


def test_config_get_remote_unknown() -> None:

    config = Config()
    with pytest.raises(KeyError):
        config.get_remote("unknown")


def test_config_get_remote(tmpdir: py.path.local) -> None:

    config = Config()
    config.read_string(f"[remote.origin]\naddress=file://{tmpdir.strpath}")
    repo = config.get_remote("origin")
    assert repo is not None
    assert isinstance(repo, storage.FileRepository)
