import pytest

from ._env import EnvSpec


def test_env_spec_validation() -> None:

    spec = EnvSpec("one+two")
    assert len(list(spec.items)) == 2


def test_env_spec_empty() -> None:

    with pytest.raises(ValueError):
        spec = EnvSpec("")
