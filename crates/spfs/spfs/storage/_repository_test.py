import stat
import pytest
import py.path

from .. import tracking
from .fs import FSDatabase, TagStorage, FSRepository, FSPayloadStorage
from ._layer import Layer
from ._manifest import Manifest, ManifestViewer


def test_find_aliases(tmpdir: py.path.local) -> None:

    tmpdir = tmpdir.join("repo")
    repo = FSRepository(tmpdir.strpath, create=True)
    with pytest.raises(ValueError):
        repo.find_aliases("not-existant")

    tmpdir.join("data", "file.txt").ensure()
    manifest = repo.commit_dir(tmpdir.join("data").strpath)
    layer = repo.create_layer(Manifest(manifest))
    repo.tags.push_tag("test-tag", layer.digest())

    assert repo.find_aliases(layer.digest().str()) == ["test-tag"]
    assert repo.find_aliases("test-tag") == [layer.digest().str()]


def test_commit_mode(tmpdir: py.path.local) -> None:

    repo = FSRepository(tmpdir.join("repo").strpath, create=True)

    datafile_path = "dir1.0/dir2.0/file.txt"
    symlink_path = "dir1.0/dir2.0/file2.txt"

    src_dir = tmpdir.join("source")
    link_dest = src_dir.join(datafile_path)
    link_dest.write("somedata", ensure=True)
    src_dir.join(symlink_path).mksymlinkto(link_dest)
    link_dest.chmod(0o444)

    manifest = repo.commit_dir(src_dir.strpath)

    assert isinstance(repo, ManifestViewer)
    rendered_dir = repo.render_manifest(Manifest(manifest))
    rendered_symlink = py.path.local(rendered_dir).join(symlink_path)
    assert stat.S_ISLNK(rendered_symlink.lstat().mode)

    symlink_entry = manifest.get_path(symlink_path)
    payloads = repo.payloads
    assert isinstance(payloads, FSPayloadStorage)
    symlink_blob = py.path.local(payloads._build_digest_path(symlink_entry.object))
    assert not stat.S_ISLNK(symlink_blob.lstat().mode)


def test_commit_broken_link(tmpdir: py.path.local) -> None:

    repo = FSRepository(tmpdir.join("repo").strpath, create=True)

    src_dir = tmpdir.join("source").ensure(dir=True)
    src_dir.join("broken-link").mksymlinkto("nonexistant")

    manifest = repo.commit_dir(src_dir.strpath)
    assert manifest.get_path("broken-link") is not None


def test_commit_dir(tmpdir: py.path.local) -> None:

    repo = FSRepository(tmpdir.join("repo").strpath, create=True)

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("file.txt").write("rootdata", ensure=True)

    manifest = Manifest(repo.commit_dir(src_dir.strpath))
    manifest2 = Manifest(repo.commit_dir(src_dir.strpath))
    assert manifest.digest() == manifest2.digest()
