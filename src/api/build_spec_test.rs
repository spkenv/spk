# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Dict, Type
import pytest

from ._option_map import OptionMap
from ._build_spec import PkgOpt, VarOpt, BuildSpec


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
                "options": [{"var": "my-opt/any-value"}],
                "variants": [{"my-opt": "any-value"}, {}],
            },
        )


def test_variants_must_be_unique_unknown_ok() -> None:

    # unreconized variant values are ok if they are unique still
    BuildSpec.from_dict(
        {"variants": [{"unknown": "any-value"}, {"unknown": "any_other_value"}]}
    )


def test_resolve_all_options_package_option() -> None:

    spec = BuildSpec.from_dict(
        {
            "options": [
                {"var": "python.abi/cp37m"},
                {"var": "my-opt/default"},
                {"var": "debug/off"},
            ]
        }
    )

    options = OptionMap(
        {
            "python.abi": "cp27mu",
            "my-opt": "value",
            "my-pkg.my-opt": "override",
            "debug": "on",
        }
    )
    resolved = spec.resolve_all_options("my-pkg", options)
    assert resolved["my-opt"] == "override", "namespaced option should take precedence"
    assert resolved["debug"] == "on", "global opt should resolve if given"
    assert resolved["python.abi"] == "cp27mu", "opt for other package should exist"
