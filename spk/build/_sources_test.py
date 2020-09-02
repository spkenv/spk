import os
import tarfile

import py.path
import pytest
import spfs

from .. import api
from ._sources import validate_source_changeset, CollectionError, collect_sources


def test_validate_sources_changeset_nothing() -> None:

    with pytest.raises(CollectionError):

        validate_source_changeset([], "/spfs")


def test_validate_sources_changeset_not_in_dir() -> None:

    with pytest.raises(CollectionError):

        validate_source_changeset(
            [spfs.tracking.Diff(path="/file.txt", mode=spfs.tracking.DiffMode.changed)],
            "/some/dir",
        )


def test_validate_sources_changeset_ok() -> None:

    validate_source_changeset(
        [
            spfs.tracking.Diff(
                path="/some/dir/file.txt", mode=spfs.tracking.DiffMode.added
            )
        ],
        "/some/dir",
    )


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
    dir_source = api.LocalSource.from_dict({"path": source_dir, "subdir": "local"})
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
    assert dest_dir.join("local/file.txt").isfile()
    assert dest_dir.join("local/source_file.txt").isfile()
