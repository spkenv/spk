from typing import List
import pytest

from .. import api
from ._repository import Repository, VersionExistsError
from ._mem import MemRepository


def test_repo_list_emtpy(repo: Repository) -> None:

    assert repo.list_packages() == [], "should not fail when empty"


def test_repo_list_package_versions_empty(repo: Repository) -> None:

    assert (
        repo.list_package_versions("nothing") == []
    ), "should not fail with unknown package"


def test_repo_publish_spec(repo: Repository) -> None:

    spec = api.Spec.from_dict({"pkg": "my_pkg/1.0.0",})
    repo.publish_spec(spec)
    assert repo.list_packages() == ["my_pkg"]
    assert repo.list_package_versions("my_pkg") == ["1.0.0"]

    with pytest.raises(VersionExistsError):
        repo.publish_spec(spec)
