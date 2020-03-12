from typing import NamedTuple, Set, Optional
from datetime import datetime

import pytz.reference
import structlog

from . import storage, tracking


_LOGGER = structlog.get_logger("spfs.prune")


class PruneParameters:
    """Specifies a range of conditions for pruning tags out of a repository."""

    def __init__(
        self,
        prune_if_older_than: datetime = None,
        keep_if_newer_than: datetime = None,
        prune_if_version_more_than: int = None,
        keep_if_version_less_than: int = None,
    ):

        if prune_if_older_than and prune_if_older_than.tzinfo is None:
            prune_if_older_than = prune_if_older_than.astimezone(
                pytz.reference.LocalTimezone()
            )
        if keep_if_newer_than and keep_if_newer_than.tzinfo is None:
            keep_if_newer_than = keep_if_newer_than.astimezone(
                pytz.reference.LocalTimezone()
            )

        self.prune_if_older_than = prune_if_older_than
        self.keep_if_newer_than = keep_if_newer_than
        self.prune_if_version_more_than = prune_if_version_more_than
        self.keep_if_version_less_than = keep_if_version_less_than

    def should_prune(self, spec: tracking.TagSpec, tag: tracking.Tag) -> bool:

        if self.keep_if_version_less_than is not None:
            if spec.version < self.keep_if_version_less_than:
                return False
        if self.keep_if_newer_than is not None:
            if tag.time > self.keep_if_newer_than:
                return False

        if self.prune_if_version_more_than is not None:
            if spec.version > self.prune_if_version_more_than:
                return True
        if self.prune_if_older_than is not None:
            if tag.time < self.prune_if_older_than:
                return True

        return False


def get_prunable_tags(
    tags: storage.TagStorage, params: PruneParameters
) -> Set[tracking.Tag]:

    to_prune = set()
    for spec, stream in tags.iter_tag_streams():
        _LOGGER.info(f"processing tag: {spec}")
        version = -1
        for tag in stream:
            version += 1
            versioned_spec = tracking.build_tag_spec(spec.name, spec.org, version)
            if params.should_prune(versioned_spec, tag):
                to_prune.add(tag)

    return to_prune


def prune_tags(tags: storage.TagStorage, params: PruneParameters) -> Set[tracking.Tag]:

    to_prune = get_prunable_tags(tags, params)
    for tag in to_prune:
        tags.remove_tag(tag)
    return to_prune
