from typing import NamedTuple, Tuple, Iterable, Dict
from typing_extensions import Protocol, runtime_checkable
import hashlib

import json

from .. import tracking, encoding


@runtime_checkable
class TagStorage(Protocol):
    """A location where tags are tracked and persisted."""

    def has_tag(self, tag: str) -> bool:
        """Return true if the given tag exists in this storage."""
        ...

    def resolve_tag(self, tag_spec: str) -> tracking.Tag:
        """Return the digest identified by the given tag spec.

        Raises:
            ValueError: if the tag does not exist in this storage
        """
        ...

    def ls_tags(self, path: str) -> Iterable[str]:
        """List tags and tag directories based on a tag path.

        For example, if the repo contains the following tags:
          spi/stable/my_tag
          spi/stable/other_tag
          spi/latest/my_tag
        Then ls_tags("spi") would return:
          stable
          latest
        """
        ...

    def find_tags(self, digest: encoding.Digest) -> Iterable[tracking.TagSpec]:
        """Find tags that point to the given digest."""
        ...

    def iter_tags(self) -> Iterable[Tuple[tracking.TagSpec, tracking.Tag]]:
        """Iterate through the available tags in this storage."""
        ...

    def iter_tag_streams(
        self,
    ) -> Iterable[Tuple[tracking.TagSpec, Iterable[tracking.Tag]]]:
        """Iterate through the available tags in this storage."""
        ...

    def read_tag(self, tag: str) -> Iterable[tracking.Tag]:
        """Read the entire tag stream for the given tag.

        Raises:
            ValueError: if the tag does not exist in the storage
        """
        ...

    def push_tag(self, tag: str, target: encoding.Digest) -> tracking.Tag:
        """Push the given tag onto the tag stream."""
        ...

    def push_raw_tag(self, tag: tracking.Tag) -> None:
        """Push the given tag data to the tag stream, regardless of if it's valid."""
        ...

    def remove_tag_stream(self, tag: str) -> None:
        """Remove an entire tag and all related tag history."""
        ...

    def remove_tag(self, tag: tracking.Tag) -> None:
        """Remove the oldest stored instance of the given tag."""
        ...
