from typing import Mapping, Union, Type

import pytest

from ._env import expand_vars, expand_defined_vars


@pytest.mark.parametrize(
    "value,vars,expected",
    [
        ("$NOTHING", {}, "$NOTHING"),
        ("NOTHING", {"NOTHING": "something"}, "NOTHING"),
        ("$NOTHING:$SOMETHING", {"SOMETHING": "something"}, "$NOTHING:something"),
        ("$SOMETHING:$NOTHING", {"SOMETHING": "something"}, "something:$NOTHING"),
        ("${SOMETHING}$NOTHING", {"SOMETHING": "something"}, "something$NOTHING"),
    ],
)
def test_expand_defined_args(
    value: str, vars: Mapping[str, str], expected: str
) -> None:

    assert expand_defined_vars(value, vars) == expected


@pytest.mark.parametrize(
    "value,vars,expected",
    [
        ("$NOTHING", {}, KeyError),
        ("NOTHING", {}, "NOTHING"),
        ("$NOTHING:$SOMETHING", {"SOMETHING": "something"}, KeyError),
        ("$SOMETHING:other", {"SOMETHING": "something"}, "something:other"),
        ("other${SOMETHING}other", {"SOMETHING": "something"}, "othersomethingother"),
    ],
)
def test_expand_vars(
    value: str, vars: Mapping[str, str], expected: Union[str, Type[Exception]]
) -> None:

    if isinstance(expected, str):
        assert expand_vars(value, vars) == expected
    else:
        with pytest.raises(expected):
            expand_vars(value, vars)
