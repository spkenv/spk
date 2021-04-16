# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import io

import pytest

from ._ident import Ident
from ._build_spec import BuildSpec
from ._spec import Spec, read_spec, InstallSpec


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

    _spec = read_spec(io.StringIO())


def test_install_embedded_build_options() -> None:

    InstallSpec.from_dict(
        {
            "embedded": [
                {
                    "pkg": "embedded/1.0.0",
                    "build": {"options": [{"var": "python.abi", "static": "cp37"}]},
                }
            ]
        }
    )

    with pytest.raises(ValueError):
        InstallSpec.from_dict(
            {
                "embedded": [
                    {
                        "pkg": "embedded/1.0.0",
                        "build": {"script": "echo hello"},
                    }
                ]
            }
        )
