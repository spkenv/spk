import pytest

from ._build import parse_build, SRC
from ._option_map import OptionMap


def test_parse_build_src() -> None:

    # should allow non-digest if it's the src token
    parse_build(SRC)


def test_parse_build() -> None:

    parse_build(OptionMap().digest())

    with pytest.raises(ValueError):
        parse_build("not eight characters")
    with pytest.raises(ValueError):
        parse_build("invalid.")
