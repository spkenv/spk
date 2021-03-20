import pytest

import spkrs

from .. import api
from ._repository import Repository, VersionExistsError, PackageNotFoundError


def test_repo_list_emtpy(repo: Repository) -> None:

    assert repo.list_packages() == [], "should not fail when empty"


def test_repo_list_package_versions_empty(repo: Repository) -> None:

    assert (
        list(repo.list_package_versions("nothing")) == []
    ), "should not fail with unknown package"


def test_repo_list_package_builds_empty(repo: Repository) -> None:

    assert (
        list(repo.list_package_builds("nothing/1.0.0")) == []
    ), "should not fail with unknown package"


def test_repo_read_spec_empty(repo: Repository) -> None:

    with pytest.raises(PackageNotFoundError):
        repo.read_spec(api.parse_ident("nothing"))


def test_repo_get_package_empty(repo: Repository) -> None:

    with pytest.raises(PackageNotFoundError):
        repo.get_package(api.parse_ident("nothing/1.0.0/src"))


def test_repo_publish_spec(repo: Repository) -> None:

    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0"})
    repo.publish_spec(spec)
    assert list(repo.list_packages()) == ["my-pkg"]
    assert list(repo.list_package_versions("my-pkg")) == ["1.0.0"]

    with pytest.raises(VersionExistsError):
        repo.publish_spec(spec)
    repo.force_publish_spec(spec)


def test_repo_publish_package(repo: Repository) -> None:

    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0"})
    repo.publish_spec(spec)
    spec.pkg.set_build("7CI5R7Y4")
    repo.publish_package(spec, spkrs.EMPTY_DIGEST)
    assert list(repo.list_package_builds(spec.pkg)) == [spec.pkg]
    assert repo.read_spec(spec.pkg) == spec


def test_repo_remove_package(repo: Repository) -> None:

    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0"})
    repo.publish_spec(spec)
    spec.pkg.set_build("7CI5R7Y4")
    repo.publish_package(spec, spkrs.EMPTY_DIGEST)
    assert list(repo.list_package_builds(spec.pkg)) == [spec.pkg]
    assert repo.read_spec(spec.pkg) == spec
    repo.remove_package(spec.pkg)
    assert list(repo.list_package_builds(spec.pkg)) == []
