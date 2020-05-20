from typing import Mapping

import pytest

from ._env import expand_vars, expand_defined_vars


@pytest.mark.parametrize(
    "value,vars,expected",
    [
        ("$NOTHING", {}, "$NOTHING"),
        ("$NOTHING:$SOMETHING", {"SOMETHING": "something"}, "$NOTHING:something"),
        ("$SOMETHING:$NOTHING", {"SOMETHING": "something"}, "something:$NOTHING"),
    ],
)
def test_expand_defined_args(
    value: str, vars: Mapping[str, str], expected: str
) -> None:

    assert expand_defined_vars(value, vars) == expected
