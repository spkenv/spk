import pytest

import spfs

from ._sources import validate_source_changeset, CollectionError


def test_validate_sources_changeset_nothing() -> None:

    with pytest.raises(CollectionError):

        validate_source_changeset([], "/spfs")


def test_validate_sources_changeset_not_in_dir() -> None:

    with pytest.raises(CollectionError):

        validate_source_changeset(
            [spfs.tracking.Diff(path="/file.txt", mode=spfs.tracking.DiffMode.changed)],
            "/some/dir",
        )


def test_validate_sources_changeset_ok() -> None:

    validate_source_changeset(
        [
            spfs.tracking.Diff(
                path="/some/dir/file.txt", mode=spfs.tracking.DiffMode.added
            )
        ],
        "/some/dir",
    )
