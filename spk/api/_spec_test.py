from ._ident import Ident
from ._build_spec import BuildSpec
from ._spec import Spec

from ruamel import yaml


def test_spec_from_dict() -> None:

    spec = Spec.from_dict(
        {"pkg": "hello_world/1.0.0", "depends": [{"pkg": "output/0.1.9"}]}
    )
    assert isinstance(spec.build, BuildSpec)
    assert isinstance(spec.pkg, Ident)
