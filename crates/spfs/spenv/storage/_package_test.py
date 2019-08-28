import os

import py.path
import pytest

from ._package import Package, PackageStorage, _ensure_package


def test_package_properties(tmpdir: py.path.local) -> None:

    package = Package(tmpdir.strpath)
    assert tmpdir.bestrelpath(package.rootdir) == "."
    assert os.path.basename(package.diffdir) == Package._diffdir
    assert os.path.basename(package.metadir) == Package._metadir


def test_list_no_packages(tmpdir: py.path.local) -> None:

    storage = PackageStorage(tmpdir.strpath)
    assert storage.list_packages() == []


def test_list_no_storage() -> None:

    storage = PackageStorage("/tmp/doesnotexist  ")
    assert storage.list_packages() == []


def test_remove_no_package(tmpdir: py.path.local) -> None:

    storage = PackageStorage(tmpdir.strpath)
    with pytest.raises(ValueError):
        storage.remove_package("noexist")


def test_remove_package(tmpdir: py.path.local) -> None:

    storage = PackageStorage(tmpdir.strpath)
    _ensure_package(tmpdir.join("package").ensure(dir=True))
    storage.remove_package("package")
    assert not tmpdir.join("package").exists()


def test_read_package_noexist(tmpdir: py.path.local) -> None:

    storage = PackageStorage(tmpdir.strpath)
    with pytest.raises(ValueError):
        storage.read_package("noexist")


def test_read_package(tmpdir: py.path.local) -> None:

    storage = PackageStorage(tmpdir.strpath)
    storage._ensure_package("--id--")
    package = storage.read_package("--id--")
    assert isinstance(package, Package)
    assert package.ref == "--id--"


def test_commit_dir(tmpdir: py.path.local) -> None:

    storage = PackageStorage(tmpdir.join("storage").strpath)

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("file.txt").write("rootdata", ensure=True)

    package = storage.commit_dir(src_dir.strpath)
    assert py.path.local(package.rootdir).exists()

    package2 = storage.commit_dir(src_dir.strpath)
    assert package.ref == package2.ref

    src_dir.join("file.txt").write("newrootdata", ensure=True)
    package3 = storage.commit_dir(src_dir.strpath)

    assert package3.ref != package2.ref
