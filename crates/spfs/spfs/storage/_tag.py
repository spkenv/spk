from typing import NamedTuple, Tuple, Iterable, Dict
from typing_extensions import Protocol, runtime_checkable
import hashlib

import json

from .. import tracking, encoding


@runtime_checkable
class TagStorage(Protocol):
    def resolve_tag(self, tag_spec: str) -> tracking.Tag:
        """Return the digest identified by the given tag spec.

        Raises:
            ValueError: if the tag does not exist in this storage
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
