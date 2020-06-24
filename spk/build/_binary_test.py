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
    BinaryPackageBuilder,
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
                    "env | grep SPK",
                    'test ! -x "$SPK_PKG_dep"',
                    'test "$SPK_PKG_dep_VERSION" == "1.0.0"',
                    'test "$SPK_OPT_dep" == "1.0.0"',
                ],
                "options": [{"pkg": "dep", "default": "1.0.0"}],
            },
        }
    )

    tmprepo.publish_spec(dep_spec)
    BinaryPackageBuilder.from_spec(dep_spec).with_repository(tmprepo).build()
    BinaryPackageBuilder.from_spec(spec).with_repository(tmprepo).build()


def test_build_package_pinning(tmprepo: storage.SpFSRepository) -> None:

    dep_spec = api.Spec.from_dict(
        {"pkg": "dep/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    )
    spec = api.Spec.from_dict(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": ["touch /spfs/top-file",],
                "options": [{"pkg": "dep", "default": "1.0.0"}],
            },
            "install": {"requirements": [{"pkg": "dep", "pin": "~x.x"}]},
        }
    )

    tmprepo.publish_spec(dep_spec)
    BinaryPackageBuilder.from_spec(dep_spec).with_repository(tmprepo).build()
    pkg = BinaryPackageBuilder.from_spec(spec).with_repository(tmprepo).build()

    spec = tmprepo.read_spec(pkg)
    assert str(spec.install.requirements[0].pkg) == "dep/~1.0"
