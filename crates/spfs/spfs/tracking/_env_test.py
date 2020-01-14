import pytest

from ._env import EnvSpec


def test_env_spec_validation() -> None:

    spec = EnvSpec("one+two")
    assert len(spec.tags) == 2

    with pytest.raises(ValueError):
        EnvSpec("")
