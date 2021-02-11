from typing import List
from .. import api
from .graph import State
from .validation import (
    Validator,
    default_validators,
)


def test_src_package_install_requests_are_not_considered() -> None:

    validators: List[Validator] = default_validators()

    spec = api.Spec.from_dict(
        {
            "pkg": "my-pkg/1.0.0/src",
            "install": {
                "embedded": [{"pkg": "embedded/9.0.0"}],
                "requirements": [{"pkg": "dependency/=2"}, {"var": "debug/on"}],
            },
        }
    )

    state = State(
        pkg_requests=(
            api.PkgRequest.from_dict({"pkg": "my-pkg/=1.0.0/src"}),
            api.PkgRequest.from_dict({"pkg": "embedded/=1.0.0"}),
            api.PkgRequest.from_dict({"pkg": "dependency/=1"}),
        ),
        var_requests=tuple(),
        options=(("debug", "off"),),
        packages=tuple(),
    )
    for validator in validators:
        msg = "Source package should be valid regardless of requirements"
        assert validator.validate(state, spec), msg
