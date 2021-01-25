from typing import Dict, Type
import pytest

from ._build_spec import Option, PkgOpt, VarOpt, BuildSpec


@pytest.mark.parametrize(
    "spec,value,err",
    [
        ({"pkg": "my-pkg"}, "1", None),
        ({"pkg": "my-pkg"}, "none", ValueError),
        ({"pkg": "my-pkg"}, "", None),
    ],
)
def test_pkg_opt_validation(spec: Dict, value: str, err: Type[Exception]) -> None:

    opt = PkgOpt.from_dict(spec)
    if err is None:
        opt.set_value(value)
        return
    with pytest.raises(err):
        opt.set_value(value)


@pytest.mark.parametrize(
    "spec,value,err",
    [
        ({"var": "my-var", "choices": ["hello", "world"]}, "hello", None),
        ({"var": "my-var", "choices": ["hello", "world"]}, "bad", ValueError),
        ({"var": "my-var", "choices": ["hello", "world"]}, "", None),
    ],
)
def test_var_opt_validation(spec: Dict, value: str, err: Type[Exception]) -> None:

    opt = VarOpt.from_dict(spec)
    if err is None:
        opt.set_value(value)
        return
    with pytest.raises(err):
        opt.set_value(value)


def test_variants_must_be_unique() -> None:

    with pytest.raises(ValueError):

        # two variants end up resolving to the same set of options
        BuildSpec.from_dict(
            {
                "options": [{"var": "my-opt", "default": "any-value"}],
                "variants": [{"my-opt": "any-value"}, {}],
            },
        )


def test_variants_must_be_unique_unknown_ok() -> None:

    # unreconized variant values are ok if they are unique still
    BuildSpec.from_dict(
        {"variants": [{"unknown": "any-value"}, {"unknown": "any_other_value"}]}
    )
