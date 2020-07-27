import io

from ._ident import Ident
from ._build_spec import BuildSpec
from ._spec import Spec, read_spec

from ruamel import yaml


def test_spec_from_dict() -> None:

    spec = Spec.from_dict(
        {
            "pkg": "hello-world/1.0.0",
            "install": {"requirements": [{"pkg": "output/0.1.9"}]},
        }
    )
    assert isinstance(spec.build, BuildSpec)
    assert isinstance(spec.pkg, Ident)


def test_empty_spec_is_valid() -> None:

    spec = read_spec(io.StringIO())
