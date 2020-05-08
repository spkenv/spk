from ._spec import Spec

from ruamel import yaml


def test_spec_from_dict() -> None:

    Spec.from_dict({"pkg": "hello_world/1.0.0", "depends": [{"pkg": "output/0.1.9"}]})
