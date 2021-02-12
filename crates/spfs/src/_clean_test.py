from typing import List
import io
import os

import pytest
import py.path

from . import storage, tracking, graph
from ._clean import (
    get_all_attached_objects,
    get_all_unattached_objects,
    get_all_unattached_payloads,
    clean_untagged_objects,
)


def test_get_attached_objects(tmprepo: storage.fs.FSRepository) -> None:

    payload_digest = tmprepo.payloads.write_payload(io.BytesIO(b"hello, world"))
    blob = storage.Blob(payload=payload_digest, size=0)
    tmprepo.objects.write_object(blob)

    assert (
        get_all_attached_objects(tmprepo) == set()
    ), "single blob should not be attached"
    assert get_all_unattached_objects(tmprepo) == {
        blob.digest()
    }, "single blob should be unattached"


def test_get_attached_payloads(tmprepo: storage.fs.FSRepository) -> None:

    payload_digest = tmprepo.payloads.write_payload(io.BytesIO(b"hello, world"))

    assert get_all_unattached_payloads(tmprepo) == {
        payload_digest
    }, "single payload should be attached when no blob"

    blob = storage.Blob(payload=payload_digest, size=0)
    tmprepo.objects.write_object(blob)

    assert (
        get_all_unattached_payloads(tmprepo) == set()
    ), "single payload should be attached to blob"


def test_get_attached_unattached_objects_blob(
    tmpdir: py.path.local, tmprepo: storage.fs.FSRepository
) -> None:

    data_dir = tmpdir.join("data")
    data_dir.join("file.txt").write("hello, world", ensure=True)

    manifest = tmprepo.commit_dir(data_dir.strpath)
    layer = tmprepo.create_layer(storage.Manifest(manifest))
    tmprepo.tags.push_tag("my_tag", layer.digest())
    blob_digest = manifest.root["file.txt"].object

    assert blob_digest in get_all_attached_objects(
        tmprepo
    ), "blob in manifest in tag should be attached"
    assert blob_digest not in get_all_unattached_objects(
        tmprepo
    ), "blob in manifest in tag should be attached"


@pytest.mark.timeout(3)
def test_clean_untagged_objects(
    tmpdir: py.path.local, tmprepo: storage.fs.FSRepository
) -> None:

    data_dir_1 = tmpdir.join("data")
    data_dir_1.join("dir/dir/test.file").write("1 hello", ensure=True)
    data_dir_1.join("dir/dir/test.file2").write("1 hello, world", ensure=True)
    data_dir_1.join("dir/dir/test.file4").write("1 hello, world", ensure=True)
    data_dir_1.join("dir/dir/test.file4").write("1 hello, other", ensure=True)
    data_dir_1.join("dir/dir/test.file4").write("1 cleanme", ensure=True)
    data_dir_2 = tmpdir.join("data2")
    data_dir_2.join("dir/dir/test.file").write("2 hello", ensure=True)
    data_dir_2.join("dir/dir/test.file2").write("2 hello, world", ensure=True)

    manifest1 = tmprepo.commit_dir(data_dir_1.strpath)

    manifest2 = tmprepo.commit_dir(data_dir_2.strpath)
    layer = tmprepo.create_layer(storage.Manifest(manifest2))
    tmprepo.tags.push_tag("tagged_manifest", layer.digest())

    clean_untagged_objects(tmprepo)

    for _, entry in manifest1.walk():
        if entry.kind is not tracking.EntryKind.BLOB:
            continue
        with pytest.raises(graph.UnknownObjectError):
            tmprepo.payloads.open_payload(entry.object).close()

    for _, entry in manifest2.walk():
        if entry.kind is not tracking.EntryKind.BLOB:
            continue
        tmprepo.payloads.open_payload(entry.object).close()


def test_clean_untagged_objects_layers_platforms(
    tmprepo: storage.fs.FSRepository,
) -> None:

    manifest = tracking.Manifest()
    layer = tmprepo.create_layer(storage.Manifest(manifest))
    platform = tmprepo.create_platform([layer.digest()])

    clean_untagged_objects(tmprepo)

    with pytest.raises(graph.UnknownObjectError):
        tmprepo.read_layer(layer.digest())

    with pytest.raises(graph.UnknownObjectError):
        tmprepo.read_platform(platform.digest())


def test_clean_manifest_renders(
    tmpdir: py.path.local, tmprepo: storage.fs.FSRepository
) -> None:

    data_dir = tmpdir.join("data")
    data_dir.join("dir/dir/file.txt").write("hello", ensure=True)
    data_dir.join("dir/name.txt").write("john doe", ensure=True)

    manifest = tmprepo.commit_dir(data_dir.strpath)
    layer = tmprepo.create_layer(storage.Manifest(manifest))
    platform = tmprepo.create_platform([layer.digest()])
    tmprepo.render_manifest(storage.Manifest(manifest))

    files = _list_files(tmprepo.root)
    assert len(files) != 0, "should have stored data"

    clean_untagged_objects(tmprepo)

    files = _list_files(tmprepo.root)
    for filepath in files:
        try:
            digest = tmprepo.objects.get_digest_from_path(filepath)  # type: ignore
            obj = tmprepo.objects.read_object(digest)
        except:
            pass
    assert files == [
        os.path.join(tmprepo.root, "VERSION")
    ], "should remove all created data files"


def _list_files(dirname: str) -> List[str]:

    all_files: List[str] = []
    for root, _, files in os.walk(dirname):
        all_files += [os.path.join(root, f) for f in files]
    return all_files
