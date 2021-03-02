from typing import Any
import os

import pytest
import py.path

import spkrs

from .. import api, storage
from ._sources import SourcePackageBuilder, data_path
from spkrs import validate_build_changeset
from ._binary import (
    BuildError,
    BinaryPackageBuilder,
)


def test_validate_build_changeset_nothing() -> None:

    with pytest.raises(BuildError):

        validate_build_changeset([])


def test_validate_build_changeset_modified() -> None:

    with pytest.raises(BuildError):

        validate_build_changeset(
            [
                spkrs.tracking.Diff(
                    path="/spfs/file.txt", mode=spkrs.tracking.DiffMode.changed
                )
            ]
        )


def test_build_artifacts(tmpdir: py.path.local, capfd: Any, monkeypatch: Any) -> None:

    spec = api.Spec.from_dict(
        {"pkg": "test/1.0.0", "build": {"script": "echo $PWD > /dev/stderr"}}
    )

    (
        BinaryPackageBuilder()
        .from_spec(spec)
        .with_source(tmpdir.strpath)
        ._build_artifacts()
    )

    _, err = capfd.readouterr()
    assert err.strip() == tmpdir.strpath


def test_build_package_options(tmprepo: storage.SpFSRepository) -> None:

    dep_spec = api.Spec.from_dict(
        {"pkg": "dep/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    )
    spec = api.Spec.from_dict(
        {
            "pkg": "top/1.2.3+r.2",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                    "test -f /spfs/dep-file",
                    "env | grep SPK",
                    'test ! -x "$SPK_PKG_dep"',
                    'test "$SPK_PKG_dep_VERSION" == "1.0.0"',
                    'test "$SPK_OPT_dep" == "1.0.0"',
                    'test "$SPK_PKG_NAME" == "top"',
                    'test "$SPK_PKG_VERSION" == "1.2.3+r.2"',
                    'test "$SPK_PKG_VERSION_MAJOR" == "1"',
                    'test "$SPK_PKG_VERSION_MINOR" == "2"',
                    'test "$SPK_PKG_VERSION_PATCH" == "3"',
                    'test "$SPK_PKG_VERSION_BASE" == "1.2.3"',
                ],
                "options": [{"pkg": "dep"}],
            },
        }
    )

    tmprepo.publish_spec(dep_spec)
    BinaryPackageBuilder.from_spec(dep_spec).with_source(".").with_repository(
        tmprepo
    ).build()
    spec = (
        BinaryPackageBuilder.from_spec(spec)
        .with_source(".")
        .with_repository(tmprepo)
        .with_option("dep", "2.0.0")  # option should be set in final published spec
        .with_option("top.dep", "1.0.0")  # specific option takes precendence
        .build()
    )

    build_options = tmprepo.read_spec(spec.pkg).resolve_all_options(
        api.OptionMap({"dep": "7"})  # given value should be ignored after build
    )
    assert build_options.get("dep") == "~1.0.0"


def test_build_package_pinning(tmprepo: storage.SpFSRepository) -> None:

    dep_spec = api.Spec.from_dict(
        {"pkg": "dep/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    )
    spec = api.Spec.from_dict(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "options": [{"pkg": "dep", "default": "1.0.0"}],
            },
            "install": {"requirements": [{"pkg": "dep", "fromBuildEnv": "~x.x"}]},
        }
    )

    tmprepo.publish_spec(dep_spec)
    BinaryPackageBuilder.from_spec(dep_spec).with_source(os.getcwd()).with_repository(
        tmprepo
    ).build()
    spec = (
        BinaryPackageBuilder.from_spec(spec)
        .with_source(os.getcwd())
        .with_repository(tmprepo)
        .build()
    )

    spec = tmprepo.read_spec(spec.pkg)
    req = spec.install.requirements[0]
    assert isinstance(req, api.PkgRequest)
    assert str(req.pkg) == "dep/~1.0"


def test_build_package_missing_deps(tmprepo: storage.SpFSRepository) -> None:

    spec = api.Spec.from_dict(
        {
            "pkg": "dep/1.0.0",
            "build": {"script": "touch /spfs/dep-file"},
            "install": {"requirements": [{"pkg": "does-not-exist"}]},
        }
    )

    # should not fail to resolve build env and build even though
    # runtime dependency is missing in the current repos
    spec = (
        BinaryPackageBuilder.from_spec(spec)
        .with_source(os.getcwd())
        .with_repository(tmprepo)
        .build()
    )


def test_build_var_pinning(tmprepo: storage.SpFSRepository) -> None:

    dep_spec = api.Spec.from_dict(
        {
            "pkg": "dep/1.0.0",
            "build": {
                "script": "touch /spfs/dep-file",
                "options": [{"var": "depvar", "default": "depvalue"}],
            },
        }
    )
    spec = api.Spec.from_dict(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "options": [
                    {"pkg": "dep", "default": "1.0.0"},
                    {"var": "topvar", "default": "topvalue"},
                ],
            },
            "install": {
                "requirements": [
                    {"var": "topvar", "fromBuildEnv": True},
                    {"var": "dep.depvar", "fromBuildEnv": True},
                ]
            },
        }
    )

    tmprepo.publish_spec(dep_spec)
    BinaryPackageBuilder.from_spec(dep_spec).with_source(os.getcwd()).with_repository(
        tmprepo
    ).build()
    spec = (
        BinaryPackageBuilder.from_spec(spec)
        .with_source(os.getcwd())
        .with_repository(tmprepo)
        .build()
    )

    spec = tmprepo.read_spec(spec.pkg)
    topreq = spec.install.requirements[0]
    assert isinstance(topreq, api.VarRequest)
    assert str(topreq.value) == "topvalue"
    depreq = spec.install.requirements[1]
    assert isinstance(depreq, api.VarRequest)
    assert str(depreq.value) == "depvalue"


def test_build_bad_options() -> None:

    spec = api.Spec.from_dict(
        {
            "pkg": "my-package/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "options": [{"var": "debug", "choices": ["on", "off"]}],
            },
        }
    )

    with pytest.raises(ValueError):
        spec = (
            BinaryPackageBuilder.from_spec(spec)
            .with_source(os.getcwd())
            .with_option("debug", "false")
            .build()
        )


def test_build_package_source_cleanup(tmprepo: storage.SpFSRepository) -> None:

    spec = api.Spec.from_dict(
        {
            "pkg": "spk-test/1.0.0+beta.1",
            "sources": [
                {"path": os.getcwd() + "/.spdev.yaml"},
                {"path": os.getcwd() + "/examples", "subdir": "examples"},
            ],
            "build": {
                "script": [
                    "ls -la",
                    "mkdir build",
                    "touch build/some_build_file.out",
                    "touch examples/some_build_file.out",
                    "mkdir examples/build",
                    "touch examples/build/some_build_file.out",
                ]
            },
        }
    )
    tmprepo.publish_spec(spec)
    src_pkg = (
        SourcePackageBuilder.from_spec(spec).with_target_repository(tmprepo).build()
    )

    pkg = BinaryPackageBuilder.from_spec(spec).with_repository(tmprepo).build()

    digest = storage.local_repository().get_package(pkg.pkg)
    spfs_repo = storage.local_repository().as_spfs_repo()
    layer = spfs_repo.read_layer(digest)
    manifest = spfs_repo.read_manifest(layer.manifest).unlock()

    source_dir_files = manifest.list_dir(data_path(src_pkg, prefix=""))
    assert not source_dir_files, "no files should be committed from source path"


def test_build_package_requirement_propagation(tmprepo: storage.SpFSRepository) -> None:

    base_spec = api.Spec.from_dict(
        {
            "pkg": "base/1.0.0",
            "sources": [],
            "build": {
                "options": [
                    {"var": "inherited", "default": "val", "inheritance": "Strong"}
                ],
                "script": "echo building...",
            },
        }
    )
    top_spec = api.Spec.from_dict(
        {
            "pkg": "top/1.0.0",
            "sources": [],
            "build": {"options": [{"pkg": "base"}], "script": "echo building..."},
        }
    )
    tmprepo.publish_spec(base_spec)
    tmprepo.publish_spec(top_spec)

    SourcePackageBuilder.from_spec(base_spec).with_target_repository(tmprepo).build()
    base_pkg = (
        BinaryPackageBuilder.from_spec(base_spec).with_repository(tmprepo).build()
    )

    SourcePackageBuilder.from_spec(top_spec).with_target_repository(tmprepo).build()
    top_pkg = BinaryPackageBuilder.from_spec(top_spec).with_repository(tmprepo).build()

    assert len(top_pkg.build.options) == 2, "should get option added"
    opt = top_pkg.build.options[1]
    assert isinstance(opt, api.VarOpt), "should be given inherited option"
    assert opt.var == "base.inherited", "should be inherited as package option"
    assert (
        opt.inheritance is api.Inheritance.weak
    ), "inherited option should have weak inheritance"

    assert len(top_pkg.install.requirements) == 1, "should get install requirement"
    req = top_pkg.install.requirements[0]
    assert isinstance(req, api.VarRequest), "should be given var request"
    assert req.var == "base.inherited", "should be inherited with package namespace"
    assert not req.pin, "should not be pinned after build"
    assert req.value == "val", "should be rendered to build time var"
