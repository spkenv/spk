import py.path

from ... import tracking
from .. import Manifest
from ._database import FSPayloadStorage
from ._renderer import FSManifestViewer, _was_render_completed, _copy_manifest
from ._repository import FSRepository


def test_render_manifest(tmpdir: py.path.local) -> None:

    storage = FSPayloadStorage(tmpdir.join("storage").strpath)
    viewer = FSManifestViewer(tmpdir.join("renders").strpath, storage)

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("file.txt").write("rootdata", ensure=True)

    manifest = tracking.compute_manifest(src_dir.strpath)

    for path, entry in manifest.walk_abs(src_dir.strpath):
        if entry.kind is tracking.EntryKind.BLOB:
            with open(path, "rb") as f:
                storage.write_payload(f)

    expected = Manifest(manifest)
    rendered_path = viewer.render_manifest(expected)
    actual = Manifest(tracking.compute_manifest(rendered_path))
    assert actual.digest() == expected.digest()


def test_copy_manfest(tmpdir: py.path.local) -> None:

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("dir2.0/file2.txt").mksymlinkto("file.txt")  # type: ignore
    src_dir.join("dir2.0/abssrc").mksymlinkto(src_dir.strpath)  # type: ignore
    src_dir.join("dir2.0").chmod(0o555)
    src_dir.join("file.txt").write("rootdata", ensure=True)
    src_dir.join("file.txt").chmod(0o400)

    expected = Manifest(tracking.compute_manifest(src_dir.strpath))

    dst_dir = tmpdir.join("dest")
    _copy_manifest(expected, src_dir.strpath, dst_dir.strpath)

    actual = Manifest(tracking.compute_manifest(dst_dir.strpath))

    assert actual.digest() == expected.digest()


def test_render_manifest_with_repo(
    tmpdir: py.path.local, tmprepo: FSRepository
) -> None:

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("file.txt").write("rootdata", ensure=True)

    manifest = Manifest(tmprepo.commit_dir(src_dir.strpath))

    assert isinstance(tmprepo, FSManifestViewer)
    render = py.path.local(tmprepo._build_digest_path(manifest.digest()))
    assert not render.exists()
    tmprepo.render_manifest(manifest)
    assert render.exists()
    assert _was_render_completed(render.strpath)
    rendered_manifest = Manifest(tracking.compute_manifest(render.strpath))
    assert rendered_manifest.digest() == manifest.digest()
