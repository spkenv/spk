import io

from . import storage, tracking
from ._clean import (
    get_all_attached_objects,
    get_all_unattached_objects,
    clean_untagged_objects,
)


def test_get_attached_unattached_objects(tmprepo: storage.fs.Repository) -> None:

    blob_digest = tmprepo.write_blob(io.BytesIO(b"hello, world"))

    assert (
        get_all_attached_objects(tmprepo) == set()
    ), "single blob should not be attached"
    assert get_all_unattached_objects(tmprepo) == {
        blob_digest
    }, "single blob should be unattached"

    manifest = tracking.compute_manifest(tmprepo.root)
    layer = storage.Layer(manifest)
    tmprepo.write_layer(layer)
    tmprepo.push_tag("my_tag", layer.digest)

    assert blob_digest in get_all_attached_objects(
        tmprepo
    ), "blob in manifest in tag should be attached"
    assert blob_digest not in get_all_unattached_objects(
        tmprepo
    ), "blob in manifest in tag should be attached"
