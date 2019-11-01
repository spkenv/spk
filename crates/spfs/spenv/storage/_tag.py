from typing import NamedTuple, Tuple, Iterable, Dict
from typing_extensions import Protocol, runtime_checkable
import hashlib

import json

from .. import tracking


@runtime_checkable
class TagStorage(Protocol):
    def resolve_tag(self, tag_spec: str) -> tracking.Tag:
        """Return the digest identified by the given tag spec.

        Raises:
            ValueError: if the tag does not exist in this storage
        """
        ...

    def read_tag(self, tag: str) -> Iterable[tracking.Tag]:
        """Read the entire tag stream for the given tag.

        Raises:
            ValueError: if the tag does not exist in the storage
        """
        ...

    def push_tag(self, tag: str, target: str) -> tracking.Tag:
        """Push the given tag onto the tag stream."""
        ...
