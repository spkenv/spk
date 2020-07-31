import pytest

from .. import api


from ._spcomp2 import _build_to_options


@pytest.mark.parametrize(
    "build,expected",
    [
        (
            "rhel7-gcc63-boost170",
            {
                "centos": "7",
                "distro": "centos",
                "os": "linux",
                "gcc": "6.3",
                "boost": "1.70",
                "arch": "x86_64",
            },
        ),
        (
            "rhel40-gcc402m64",
            {
                "rhel": "40",
                "distro": "rhel",
                "os": "linux",
                "gcc": "4.02",
                "arch": "x86_64",
            },
        ),
        (
            "rhel7-gcc63-ice36",
            {
                "centos": "7",
                "distro": "centos",
                "os": "linux",
                "gcc": "6.3",
                "arch": "x86_64",
                "ice": "3.6",
            },
        ),
        (
            "spinux1-gcc44m64",
            {
                "spinux": "1",
                "distro": "spinux",
                "os": "linux",
                "gcc": "4.4",
                "arch": "x86_64",
            },
        ),
        (
            "rhel40-gcc34",
            {
                "rhel": "40",
                "distro": "rhel",
                "os": "linux",
                "gcc": "3.4",
                "arch": "x86_64",
            },
        ),
        (
            "rhel40-gcc42m64",
            {
                "rhel": "40",
                "distro": "rhel",
                "os": "linux",
                "gcc": "4.2",
                "arch": "x86_64",
            },
        ),
        (
            "spinux1-gcc412m64",
            {
                "spinux": "1",
                "distro": "spinux",
                "os": "linux",
                "gcc": "4.12",
                "arch": "x86_64",
            },
        ),
        (
            "rhel40-gcc34m64",
            {
                "rhel": "40",
                "distro": "rhel",
                "os": "linux",
                "gcc": "3.4",
                "arch": "x86_64",
            },
        ),
        (
            "rhel7-gcc48m64-ice36",
            {
                "centos": "7",
                "distro": "centos",
                "os": "linux",
                "gcc": "4.8",
                "arch": "x86_64",
                "ice": "3.6",
            },
        ),
        (
            "spinux1-gcc41m64",
            {
                "spinux": "1",
                "distro": "spinux",
                "os": "linux",
                "gcc": "4.1",
                "arch": "x86_64",
            },
        ),
    ],
)
def test_build_to_options(build: str, expected: api.OptionMap) -> None:

    actual = _build_to_options(build)
    spec = api.BuildSpec(options=actual)
    compat = spec.validate_options(expected)
    assert compat, compat
