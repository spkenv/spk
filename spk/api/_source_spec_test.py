import tarfile
import os

import py.path

from ._source_spec import GitSource, TarSource


def test_git_sources(tmpdir: py.path.local) -> None:

    tmpdir = tmpdir.join("source").ensure(dir=1)
    spec = {
        "git": os.getcwd(),
    }
    source = GitSource.from_dict(spec)
    source.collect(tmpdir.strpath)

    assert os.listdir(tmpdir.strpath)


def test_tar_sources(tmpdir: py.path.local) -> None:

    filename = tmpdir.join("archive.tar.gz").strpath
    with tarfile.open(filename, "w") as tar:
        tar.add("spk/__init__.py")

    tmpdir = tmpdir.join("source").ensure(dir=1)
    spec = {
        "tar": filename,
    }
    source = TarSource.from_dict(spec)
    source.collect(tmpdir.strpath)

    assert tmpdir.join("spk", "__init__.py").exists()
