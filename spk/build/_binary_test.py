import spkrs
from typing import Any
import subprocess
import os

import pytest
import py.path

from .. import api, storage
from ._sources import SourcePackageBuilder, data_path
from ._binary import BinaryPackageBuilder


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
                "options": [{"pkg": "dep/1.0.0"}],
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
                "options": [{"var": "depvar/depvalue"}],
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
                    {"pkg": "dep/1.0.0"},
                    {"var": "topvar/topvalue"},
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
    out = subprocess.check_output(
        ["spfs", "ls", str(digest), data_path(src_pkg, prefix="")]
    )
    assert not out, "no files should be committed from source path"


def test_build_package_requirement_propagation(tmprepo: storage.SpFSRepository) -> None:

    open("/spfs/file.txt", "w+").close()
    digest1 = spkrs.commit_layer(spkrs.active_runtime())
    spkrs.reconfigure_runtime(editable=True)
    open("/spfs/file2.txt", "w+").close()
    digest2 = spkrs.commit_layer(spkrs.active_runtime())

    base_spec = api.Spec.from_dict(
        {
            "pkg": "base/1.0.0",
            "sources": [],
            "build": {
                "options": [
                    {"var": "strong/val", "inheritance": "Strong"},
                    {"pkg": "strong-pkg/0.0.0", "inheritance": "Strong"},
                    {"var": "build/val", "inheritance": "StrongForBuildOnly"},
                    {"pkg": "build-pkg/0.0.0", "inheritance": "StrongForBuildOnly"},
                ],
                "script": "echo building...",
            },
            "install": {
                "requirements": [
                    {"pkg": "strong-pkg"},
                    {"pkg": "build-pkg"}
                ]
            }
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
    strong_dep = api.Spec.from_dict({"pkg":"strong-pkg"})
    strong_dep.update_for_build(api.OptionMap(), [])
    tmprepo.publish_package(strong_dep, digest1)
    build_dep = api.Spec.from_dict({"pkg":"build-pkg"})
    build_dep.update_for_build(api.OptionMap(), [])
    tmprepo.publish_package(build_dep, digest2)

    SourcePackageBuilder.from_spec(base_spec).with_target_repository(tmprepo).build()
    base_pkg = (
        BinaryPackageBuilder.from_spec(base_spec).with_repository(tmprepo).build()
    )

    SourcePackageBuilder.from_spec(top_spec).with_target_repository(tmprepo).build()
    top_pkg = BinaryPackageBuilder.from_spec(top_spec).with_repository(tmprepo).build()

    assert len(top_pkg.build.options) == 5, "should get options added"
    opt = top_pkg.build.options[1]
    assert isinstance(opt, api.VarOpt), "should be given inherited option"
    assert opt.var == "base.strong", "should be inherited as package option"
    assert (
        opt.inheritance is api.Inheritance.weak
    ), "inherited option should have weak inheritance"

    assert len(top_pkg.install.requirements) == 2, "should get install requirements"
    req = top_pkg.install.requirements[0]
    assert isinstance(req, api.VarRequest), "should be given var request"
    assert req.var == "base.strong", "should be inherited with package namespace"
    assert not req.pin, "should not be pinned after build"
    assert req.value == "val", "should be rendered to build time var"
