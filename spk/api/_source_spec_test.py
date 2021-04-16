# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import tarfile
import os

import py.path

from ._source_spec import GitSource, TarSource, LocalSource, ScriptSource


def test_local_source_dir(tmpdir: py.path.local) -> None:

    source_dir = tmpdir.join("source")
    source_dir.join("file.txt").ensure()
    spec = {"path": source_dir}
    source = LocalSource.from_dict(spec)
    source.collect(tmpdir.join("dest").ensure(dir=1).strpath)

    assert tmpdir.join("dest", "file.txt").exists()


def test_local_source_file(tmpdir: py.path.local) -> None:

    source_file = tmpdir.join("src", "source.txt").ensure()
    spec = {"path": source_file.strpath}
    source = LocalSource.from_dict(spec)
    source.collect(tmpdir.join("dest").ensure(dir=1).strpath)

    assert tmpdir.join("dest", "source.txt").exists()


def test_git_sources(tmpdir: py.path.local) -> None:

    tmpdir = tmpdir.join("source").ensure(dir=1)
    spec = {"git": os.getcwd()}
    source = GitSource.from_dict(spec)
    source.collect(tmpdir.strpath)

    assert os.listdir(tmpdir.strpath)


def test_tar_sources(tmpdir: py.path.local) -> None:

    filename = tmpdir.join("archive.tar.gz").strpath
    with tarfile.open(filename, "w") as tar:
        tar.add("spk/__init__.py")

    tmpdir = tmpdir.join("source").ensure(dir=1)
    spec = {"tar": filename}
    source = TarSource.from_dict(spec)
    source.collect(tmpdir.strpath)

    assert tmpdir.join("spk", "__init__.py").exists()


def test_script_sources(tmpdir: py.path.local) -> None:

    spec = {"script": ["mkdir spk", "touch spk/__init__.py"]}
    source = ScriptSource.from_dict(spec)
    source.collect(tmpdir.strpath)

    assert tmpdir.join("spk", "__init__.py").exists()
