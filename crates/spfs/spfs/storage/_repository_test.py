import stat
import pytest
import py.path

from .. import tracking
from .fs import FileDB, TagStorage, Repository, FSPayloadStorage
from ._layer import Layer
from ._manifest import ManifestViewer


def test_find_aliases(tmpdir: py.path.local) -> None:

    repo = Repository(tmpdir.strpath)
    with pytest.raises(ValueError):
        repo.find_aliases("not-existant")

    tmpdir.join("data", "file.txt").ensure()
    manifest = repo.commit_dir(tmpdir.join("data").strpath)
    repo.objects.write_object(manifest)
    layer = repo.commit_manifest(manifest)
    repo.tags.push_tag("test-tag", layer.digest())

    assert repo.find_aliases(layer.digest().str()) == ["test-tag"]
    assert repo.find_aliases("test-tag") == [layer.digest().str()]


def test_commit_mode(tmpdir: py.path.local) -> None:

    repo = Repository(tmpdir.join("repo").strpath)

    datafile_path = "dir1.0/dir2.0/file.txt"
    symlink_path = "dir1.0/dir2.0/file2.txt"

    src_dir = tmpdir.join("source")
    link_dest = src_dir.join(datafile_path)
    link_dest.write("somedata", ensure=True)
    src_dir.join(symlink_path).mksymlinkto(link_dest)
    link_dest.chmod(0o444)

    manifest = repo.commit_dir(src_dir.strpath)

    assert isinstance(repo, ManifestViewer)
    rendered_dir = repo.render_manifest(manifest)
    rendered_symlink = py.path.local(rendered_dir).join(symlink_path)
    assert stat.S_ISLNK(rendered_symlink.lstat().mode)

    symlink_entry = manifest.get_path(symlink_path)
    assert symlink_entry is not None
    payloads = repo.payloads
    assert isinstance(payloads, FSPayloadStorage)
    symlink_blob = py.path.local(payloads._build_digest_path(symlink_entry.object))
    assert not stat.S_ISLNK(symlink_blob.lstat().mode)


def test_commit_broken_link(tmpdir: py.path.local) -> None:

    repo = Repository(tmpdir.join("repo").strpath)

    src_dir = tmpdir.join("source").ensure(dir=True)
    src_dir.join("broken-link").mksymlinkto("nonexistant")

    manifest = repo.commit_dir(src_dir.strpath)
    assert manifest.get_path("broken-link") is not None


def test_commit_dir(tmpdir: py.path.local) -> None:

    repo = Repository(tmpdir.join("repo").strpath)

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("file.txt").write("rootdata", ensure=True)

    manifest = repo.commit_dir(src_dir.strpath)
    manifest2 = repo.commit_dir(src_dir.strpath)
    assert manifest.digest() == manifest2.digest()


@pytest.mark.skip("not implemented")
def test_render_manifest() -> None:

    pass
    # view = repo.payloads
    # assert isinstance(view, ManifestViewer)
    # render_path
    # render = tmpdir.join(
    #     "repo", "renders", manifest.digest().str()[:2], manifest.digest().str()[2:]
    # )
    # assert render.exists()
    # assert _was_render_completed(render.strpath)
    # rendered_manifest = tracking.compute_manifest(render.strpath)
    # assert rendered_manifest.digest() == manifest.digest()
