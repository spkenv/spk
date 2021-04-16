# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import pytest

from . import storage, api
from ._global import save_spec, load_spec


def test_load_spec_local() -> None:

    spec = api.Spec.from_dict({"pkg": "my-pkg"})
    repo = storage.local_repository()
    repo.publish_spec(spec)

    actual = load_spec("my-pkg")
    assert actual == spec


def test_save_spec() -> None:

    spec = api.Spec.from_dict({"pkg": "my-pkg"})
    repo = storage.local_repository()

    with pytest.raises(storage.PackageNotFoundError):
        repo.read_spec(spec.pkg)

    save_spec(spec)

    assert repo.read_spec(spec.pkg) is not None
