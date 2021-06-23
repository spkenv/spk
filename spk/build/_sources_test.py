import os
import tarfile
from typing import Any

import py.path

from .. import api
from ._sources import collect_sources


def test_sources_subdir(tmpdir: py.path.local) -> None:

    tar_file = tmpdir.join("archive.tar.gz").strpath
    with tarfile.open(tar_file, "w") as tar:
        tar.add("spk/__init__.py")

    tar_source = api.TarSource.from_dict(
        # purposfully add leading slash to make sure it doesn't fail
        {"tar": tar_file, "subdir": "/archive/src"}
    )
    git_source = api.GitSource.from_dict({"git": os.getcwd(), "subdir": "git_repo"})
    source_dir = tmpdir.join("source")
    source_dir.join("file.txt").ensure()
    source_dir.join(".git/gitfile").ensure()
    dir_source = api.LocalSource.from_dict({"path": source_dir.strpath, "subdir": "local"})
    source_file = tmpdir.join("src", "source_file.txt").ensure()
    file_source = api.LocalSource.from_dict(
        {"path": source_file.strpath, "subdir": "local"}
    )

    dest_dir = tmpdir.join("dest")
    spec = api.Spec()
    spec.sources = [git_source, tar_source, file_source, dir_source]
    collect_sources(spec, dest_dir.strpath)
    assert dest_dir.join("local").isdir()
    assert dest_dir.join("git_repo").isdir()
    assert dest_dir.join("archive/src").isdir()
    assert dest_dir.join("archive/src/spk/__init__.py").isfile()
    assert dest_dir.join("git_repo/spk/__init__.py").isfile()
    assert not dest_dir.join("local/.git").exists(), "should exclude git repo"
    assert dest_dir.join("local/file.txt").isfile()
    assert dest_dir.join("local/source_file.txt").isfile()


def test_sources_environment(tmpdir: py.path.local, capfd: Any) -> None:

    spec = api.Spec.from_dict({"pkg": "sources-test/0.1.0/src"})
    expected = "\n".join(
        [
            "SPK_PKG=sources-test/0.1.0/src",
            "SPK_PKG_NAME=sources-test",
            "SPK_PKG_VERSION=0.1.0",
            "SPK_PKG_BUILD=src",
            "SPK_PKG_VERSION_MAJOR=0",
            "SPK_PKG_VERSION_MINOR=1",
            "SPK_PKG_VERSION_PATCH=0",
            "SPK_PKG_VERSION_BASE=0.1.0",
        ]
    )
    script_source = api.ScriptSource.from_dict(
        {
            "script": [
                "echo SPK_PKG=${SPK_PKG}",
                "echo SPK_PKG_NAME=${SPK_PKG_NAME}",
                "echo SPK_PKG_VERSION=${SPK_PKG_VERSION}",
                "echo SPK_PKG_BUILD=${SPK_PKG_BUILD}",
                "echo SPK_PKG_VERSION_MAJOR=${SPK_PKG_VERSION_MAJOR}",
                "echo SPK_PKG_VERSION_MINOR=${SPK_PKG_VERSION_MINOR}",
                "echo SPK_PKG_VERSION_PATCH=${SPK_PKG_VERSION_PATCH}",
                "echo SPK_PKG_VERSION_BASE=${SPK_PKG_VERSION_BASE}",
            ]
        }
    )
    dest_dir = tmpdir.join("dest")
    spec.sources = [script_source]
    collect_sources(spec, dest_dir.strpath)

    out, _ = capfd.readouterr()
    assert (
        out.strip() == expected
    ), "should have access to package variables in sources script"
