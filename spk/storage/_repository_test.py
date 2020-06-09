from typing import List
import pytest

import spfs

from .. import api
from ._repository import Repository, VersionExistsError
from ._mem import MemRepository


def test_repo_list_emtpy(repo: Repository) -> None:

    assert repo.list_packages() == [], "should not fail when empty"


def test_repo_list_package_versions_empty(repo: Repository) -> None:

    assert (
        list(repo.list_package_versions("nothing")) == []
    ), "should not fail with unknown package"


def test_repo_publish_spec(repo: Repository) -> None:

    spec = api.Spec.from_dict({"pkg": "my_pkg/1.0.0",})
    repo.publish_spec(spec)
    assert list(repo.list_packages()) == ["my_pkg"]
    assert list(repo.list_package_versions("my_pkg")) == ["1.0.0"]

    with pytest.raises(VersionExistsError):
        repo.publish_spec(spec)


def test_repo_publish_package(repo: Repository) -> None:

    pkg = api.parse_ident("my_pkg/1.0.0/7CI5R7Y4")
    repo.publish_package(pkg, spfs.encoding.EMPTY_DIGEST)
    assert list(repo.list_package_builds(pkg)) == [pkg]
