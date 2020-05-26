from typing import Any
import pytest
import py.path

import spfs

from .. import api
from ._binary import validate_build_changeset, BuildError, build_artifacts


def test_validate_build_changeset_nothing() -> None:

    with pytest.raises(BuildError):

        validate_build_changeset([])


def test_validate_build_changeset_modified() -> None:

    with pytest.raises(BuildError):

        validate_build_changeset(
            [
                spfs.tracking.Diff(
                    path="/spfs/file.txt", mode=spfs.tracking.DiffMode.changed
                )
            ]
        )


def test_build_partifacts(tmpdir: py.path.local, capfd: Any, monkeypatch: Any) -> None:

    spec = api.Spec.from_dict(
        {"pkg": "test/1.0.0", "build": {"script": "echo $PWD > /dev/stderr"}}
    )

    build_artifacts(spec, tmpdir.strpath, api.OptionMap(), tmpdir.strpath)

    _, err = capfd.readouterr()
    assert err.strip() == tmpdir.strpath
