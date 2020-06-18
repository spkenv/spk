from typing import Union, Tuple, Any, MutableMapping
from dataclasses import dataclass, field
from sortedcontainers import SortedDict

VERSION_SEP = "."


class TagSet(SortedDict, MutableMapping[str, int]):
    """TagSet contains a set of pre or post release version tags."""

    pass


def parse_tag_set(tags: str) -> TagSet:
    """Parse the given string as a set of version tags.

    >>> parse_tag_set("release.0,alpha.1")
    TagSet({'alpha': 1, 'release': 0})
    """

    tag_set = TagSet()
    if not tags:
        return tag_set

    for tag in tags.split(","):
        name, num = tag.split(".")
        if name in tag_set:
            raise ValueError("duplicate tag: " + name)
        tag_set[name] = int(num)

    return tag_set


@dataclass
class Version:
    """Version specifies a package version number."""

    major: int = 0
    minor: int = 0
    patch: int = 0
    tail: Tuple[int, ...] = tuple()
    pre: TagSet = field(default_factory=TagSet)
    post: TagSet = field(default_factory=TagSet)

    def __str__(self) -> str:

        return str(VERSION_SEP.join(str(s) for s in self.parts))

    def __bool__(self) -> bool:

        return any(self.parts)

    def __lt__(self, other: Any) -> bool:

        if isinstance(other, Version):
            return self.parts < other.parts
        return bool(str(self) < other)

    def __eq__(self, other: Any) -> bool:

        if isinstance(other, Version):
            return self.parts == other.parts
        return bool(str(self) == other)

    def __gt__(self, other: Any) -> bool:

        if isinstance(other, Version):
            return self.parts > other.parts
        return bool(str(self) > other)

    def __ge__(self, other: Any) -> bool:

        if isinstance(other, Version):
            return self.parts >= other.parts
        return bool(str(self) >= other)

    def __le__(self, other: Any) -> bool:

        if isinstance(other, Version):
            return self.parts <= other.parts
        return bool(str(self) <= other)

    @property
    def parts(self) -> Tuple[int, ...]:
        return (self.major, self.minor, self.patch, *self.tail)

    def clone(self) -> "Version":

        return Version(self.major, self.minor, self.patch, self.tail)


def parse_version(version: str) -> Version:
    """Parse a string as a version specifier."""

    if not version:
        return Version()

    pre, post = "", ""
    if "+" in version:
        version, post = version.split("+", 1)
    if "-" in version:
        version, pre = version.split("-", 1)

    str_parts = version.split(VERSION_SEP)
    parts = tuple(int(p) for p in str_parts)
    return Version(  # type: ignore
        *parts[:3],  # type: ignore
        tail=parts[3:],
        pre=parse_tag_set(pre),
        post=parse_tag_set(post)
    )
