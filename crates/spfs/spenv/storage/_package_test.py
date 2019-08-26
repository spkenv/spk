from ._package import Package, PackageStorage, _ensure_package


def test_list_no_packages(tmpdir):

    storage = PackageStorage(tmpdir.strpath)
    assert storage.list_packages() == []


def test_list_no_storage():

    storage = PackageStorage("/tmp/doesnotexist  ")
    assert storage.list_packages() == []


def test_remove_no_package(tmpdir):

    storage = PackageStorage(tmpdir.strpath)
    storage.remove_package("noexist")


def test_remove_package(tmpdir):

    storage = PackageStorage(tmpdir.strpath)
    _ensure_package(tmpdir.join("package").ensure(dir=True))
    storage.remove_package("package")
    assert not tmpdir.join("package").exists()
