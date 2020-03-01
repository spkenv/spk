import io

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
