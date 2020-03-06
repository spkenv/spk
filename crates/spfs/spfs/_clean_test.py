import io
import os

import pytest
import py.path

from . import storage, tracking
from ._clean import (
    get_all_attached_objects,
    get_all_unattached_objects,
    clean_untagged_objects,
)


def test_get_attached_unattached_objects(tmprepo: storage.fs.Repository) -> None:

    blob_digest = tmprepo.blobs.write_blob(io.BytesIO(b"hello, world"))

    assert (
        get_all_attached_objects(tmprepo) == set()
    ), "single blob should not be attached"
    assert get_all_unattached_objects(tmprepo) == {
        blob_digest
    }, "single blob should be unattached"

    manifest = tracking.compute_manifest(tmprepo.root)
    layer = storage.Layer(manifest)
    tmprepo.layers.write_layer(layer)
    tmprepo.tags.push_tag("my_tag", layer.digest)

    assert blob_digest in get_all_attached_objects(
        tmprepo
    ), "blob in manifest in tag should be attached"
    assert blob_digest not in get_all_unattached_objects(
        tmprepo
    ), "blob in manifest in tag should be attached"


def test_clean_untagged_objects_blobs(
    tmpdir: py.path.local, tmprepo: storage.fs.Repository
) -> None:

    data_dir = tmpdir.join("data")
    data_dir.join("dir/dir/test.file").write("hello", ensure=True)

    manifest = tmprepo.blobs.commit_dir(data_dir.strpath)

    # shouldn't fail on empty repo
    clean_untagged_objects(tmprepo)

    for _, entry in manifest.walk():

        if entry.kind is not tracking.EntryKind.BLOB:
            continue

        with pytest.raises(storage.UnknownObjectError):
            tmprepo.blobs.open_blob(entry.object).close()


def test_clean_untagged_objects_layers_platforms(
    tmprepo: storage.fs.Repository,
) -> None:

    layer = tmprepo.layers.commit_manifest(tracking.Manifest())
    platform = tmprepo.platforms.commit_stack([layer.digest])

    clean_untagged_objects(tmprepo)

    with pytest.raises(storage.UnknownObjectError):
        tmprepo.layers.read_layer(layer.digest)

    with pytest.raises(storage.UnknownObjectError):
        tmprepo.platforms.read_platform(platform.digest)


def test_clean_manifest_renders(
    tmpdir: py.path.local, tmprepo: storage.fs.Repository
) -> None:

    data_dir = tmpdir.join("data")
    data_dir.join("dir/dir/file.txt").write("hello", ensure=True)
    data_dir.join("dir/name.txt").write("john doe", ensure=True)

    manifest = tmprepo.blobs.commit_dir(data_dir.strpath)
    layer = tmprepo.layers.commit_manifest(manifest)
    platform = tmprepo.platforms.commit_stack([layer.digest])

    file_count = _count_files(tmprepo.root)
    assert file_count != 0, "should have stored data"

    clean_untagged_objects(tmprepo)

    assert _count_files(tmprepo.root) == 0, "should remove all created data files"


def _count_files(dirname: str) -> int:

    file_count = 0
    for _, _, files in os.walk(dirname):
        file_count += len(files)
    return file_count
