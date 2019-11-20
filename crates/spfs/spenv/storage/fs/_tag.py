from typing import List, Iterator, Tuple
import os

from ... import tracking
from .. import UnknownObjectError


class TagStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def iter_tags(self) -> Iterator[Tuple[str, tracking.Tag]]:

        for root, _, files in os.walk(self._root):

            for filename in files:
                filepath = os.path.join(root, filename)
                tag = os.path.relpath(filepath, self._root)
                digest = self.resolve_tag(tag)
                yield (tag, digest)

    def read_tag(self, tag: str) -> Iterator[tracking.Tag]:
        """Read the entire tag stream for the given tag.

        Raises:
            ValueError: if the tag does not exist in the storage
        """

        spec = tracking.parse_tag_spec(tag)
        filepath = os.path.join(self._root, spec.path)
        try:
            with open(filepath, "rb") as f:
                # TODO: this should be more efficient and not
                # need to read the whole file to reverse it -
                # but not worth the complexity of implementation yet
                for line in reversed(f.readlines()):
                    line = line.rstrip(b"\n")
                    yield tracking.decode_tag(line)

        except FileNotFoundError:
            raise UnknownObjectError(f"Unknown tag: {tag}")

    def resolve_tag(self, tag: str) -> tracking.Tag:

        spec = tracking.parse_tag_spec(tag)
        try:
            stream = self.read_tag(tag)
            for i in range(abs(spec.version)):
                next(stream)
            return next(stream)
        except StopIteration:
            raise UnknownObjectError(f"tag or tag version does not exist {tag}")

    def push_tag(self, tag: str, target: str) -> tracking.Tag:
        """Push the given tag onto the tag stream."""

        tag_spec = tracking.parse_tag_spec(tag)
        try:
            parent = self.resolve_tag(tag).digest
        except ValueError:
            parent = ""
        new_tag = tracking.Tag(
            org=tag_spec.org, name=tag_spec.name, target=target, parent=parent
        )
        filepath = os.path.join(self._root, tag_spec.path)
        os.makedirs(os.path.dirname(filepath), exist_ok=True)
        with open(filepath, "a+b") as f:
            f.write(new_tag.encode() + b"\n")
        return new_tag
