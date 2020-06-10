from typing import Any
import pytest
import py.path

import spfs

from .. import api, storage
from ._binary import (
    validate_build_changeset,
    BuildError,
    build_artifacts,
    build_and_commit_artifacts,
    make_binary_package,
)


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


def test_build_package_options(tmprepo: storage.SpFSRepository) -> None:

    dep_spec = api.Spec.from_dict(
        {"pkg": "dep/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    )
    spec = api.Spec.from_dict(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                    "test -f /spfs/dep-file",
                    'test ! -x "$SPK_PKG_dep"',
                    'test "$SPK_PKG_dep_VERSION" == "1.0.0"',
                    "env | grep SPK",
                    'test "$SPK_OPT_dep" == "1.0.0"',
                ]
            },
            "opts": [{"pkg": "dep/1.0.0"}],
        }
    )

    tmprepo.publish_spec(dep_spec)
    make_binary_package(dep_spec, ".", api.OptionMap())
    make_binary_package(spec, ".", spec.resolve_all_options(api.OptionMap()))
