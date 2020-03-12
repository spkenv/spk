import random
from datetime import datetime

import pytest

from . import storage, tracking, encoding
from ._prune import prune_tags, get_prunable_tags, PruneParameters


def test_prunable_tags_age(tmprepo: storage.fs.FSRepository) -> None:

    old = tracking.Tag(
        org="testing",
        name="prune",
        target=encoding.NULL_DIGEST,
        parent=encoding.NULL_DIGEST,
        time=datetime.fromtimestamp(10000),
    )
    cutoff = 20000
    new = tracking.Tag(
        org="testing",
        name="prune",
        target=encoding.EMPTY_DIGEST,
        parent=encoding.EMPTY_DIGEST,
        time=datetime.fromtimestamp(30000),
    )
    tmprepo.tags.push_raw_tag(old)
    tmprepo.tags.push_raw_tag(new)

    tags = get_prunable_tags(
        tmprepo.tags,
        PruneParameters(prune_if_older_than=datetime.fromtimestamp(cutoff)),
    )
    assert old in tags
    assert new not in tags

    tags = get_prunable_tags(
        tmprepo.tags,
        PruneParameters(
            prune_if_older_than=datetime.fromtimestamp(cutoff),
            keep_if_newer_than=datetime.fromtimestamp(0),
        ),
    )
    assert old not in tags, "should prefer to keep when ambiguous"
    assert new not in tags


def test_prunable_tags_version(tmprepo: storage.fs.FSRepository) -> None:

    tag = "testing/versioned"
    tag5 = tmprepo.tags.push_tag(tag, encoding.EMPTY_DIGEST)
    tag4 = tmprepo.tags.push_tag(tag, encoding.NULL_DIGEST)
    tag3 = tmprepo.tags.push_tag(tag, encoding.EMPTY_DIGEST)
    tag2 = tmprepo.tags.push_tag(tag, encoding.NULL_DIGEST)
    tag1 = tmprepo.tags.push_tag(tag, encoding.EMPTY_DIGEST)
    tag0 = tmprepo.tags.push_tag(tag, encoding.NULL_DIGEST)

    tags = get_prunable_tags(
        tmprepo.tags, PruneParameters(prune_if_version_more_than=2),
    )
    assert tag0 not in tags
    assert tag1 not in tags
    assert tag2 not in tags
    assert tag3 in tags
    assert tag4 in tags
    assert tag5 in tags

    tags = get_prunable_tags(
        tmprepo.tags,
        PruneParameters(prune_if_version_more_than=2, keep_if_version_less_than=4),
    )
    assert tag0 not in tags
    assert tag1 not in tags
    assert tag2 not in tags
    assert tag3 not in tags, "should prefer to keep in ambiguous situation"
    assert tag4 in tags
    assert tag5 in tags


def test_prune_tags(tmprepo: storage.fs.FSRepository) -> None:

    tags = {}

    def reset() -> None:
        try:
            tmprepo.tags.remove_tag_stream("test/prune")
        except storage.UnknownReferenceError:
            pass
        for year in (2020, 2021, 2022, 2023, 2024, 2025):
            time = datetime(year=year, month=1, day=1)
            digest = random_digest()
            tag = tracking.Tag(org="test", name="prune", target=digest, time=time)
            tags[year] = tag
            tmprepo.tags.push_raw_tag(tag)

    reset()
    prune_tags(
        tmprepo.tags,
        PruneParameters(prune_if_older_than=datetime(day=1, month=1, year=2024)),
    )
    for tag in tmprepo.tags.read_tag("test/prune"):
        assert tag is not tags[2025]

    reset()
    prune_tags(
        tmprepo.tags, PruneParameters(prune_if_version_more_than=2),
    )
    for tag in tmprepo.tags.read_tag("test/prune"):
        assert tag is not tags[2025]
        assert tag is not tags[2024]
        assert tag is not tags[2023]

    reset()
    prune_tags(
        tmprepo.tags, PruneParameters(prune_if_version_more_than=-1),
    )
    with pytest.raises(storage.UnknownReferenceError):
        for tag in tmprepo.tags.read_tag("test/prune"):
            print(tag)


def random_digest() -> encoding.Digest:

    hasher = encoding.Hasher()
    hasher.update(bytes([random.randint(0, 255) for _ in range(64)]))
    return hasher.digest()
