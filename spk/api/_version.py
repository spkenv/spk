from typing import Union, Tuple, Any, MutableMapping
from dataclasses import dataclass, field
from sortedcontainers import SortedDict
from functools import total_ordering

VERSION_SEP = "."


@total_ordering  # type: ignore
class TagSet(SortedDict, MutableMapping[str, int]):
    """TagSet contains a set of pre or post release version tags."""

    def __lt__(self, other: Any) -> bool:
        """Return true if other if less than self.

        >>> TagSet({"pre": 1}) < TagSet({"pre": 2})
        True
        >>> TagSet({"pre": 0}) < TagSet({"pre": 0})
        False
        >>> TagSet({"alpha": 0}) < TagSet({"alpha": 0, "beta": 1})
        True
        >>> TagSet({}) < TagSet({"alpha": 0})
        True
        >>> TagSet({"alpha": 0}) > TagSet({})
        True
        >>> TagSet({"alpha": 0}) > TagSet({'beta': 1})
        False
        """

        if not isinstance(other, TagSet):
            raise TypeError(
                f"'<' not supported between TagSet and {type(other).__name__}"
            )

        for self_name, other_name in zip(self.keys(), other.keys()):

            if self_name != other_name:
                return bool(self_name < other_name)
            if self[self_name] != other[other_name]:
                return bool(self[self_name] < other[other_name])

        return bool(len(self) < len(other))


def parse_tag_set(tags: str) -> TagSet:
    """Parse the given string as a set of version tags.

    >>> parse_tag_set("release.0,alpha.1")
    TagSet({'alpha': 1, 'release': 0})
    >>> TagSet({'alpha': 0}) < TagSet({'alpha': 1})
    True
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

        base = str(VERSION_SEP.join(str(s) for s in self.parts))
        if self.pre:
            base += "-" + ",".join(f"{n}.{v}" for n, v in self.pre.items())
        if self.post:
            base += "+" + ",".join(f"{n}.{v}" for n, v in self.post.items())

        return base

    def __repr__(self) -> str:

        return f"Version({self.__str__()})"

    def __bool__(self) -> bool:

        return any(self.parts)

    def __lt__(self, other: Any) -> bool:

        if not isinstance(other, Version):
            return bool(str(self) < other)

        if self.parts < other.parts:
            return True
        if self.parts > other.parts:
            return False
        if self.pre:
            if not other.pre:
                return True
            bool(self.pre < other.pre)
        if other.pre:
            return False
        return bool(self.post < other.post)

    def __gt__(self, other: Any) -> bool:

        if not isinstance(other, Version):
            return bool(str(self) < other)

        if self.parts > other.parts:
            return True
        if self.parts < other.parts:
            return False
        if self.pre:
            if not other.pre:
                return False
            bool(self.pre > other.pre)
        if other.pre:
            return True
        return bool(self.post > other.post)

    def __eq__(self, other: Any) -> bool:

        if isinstance(other, Version):
            return (
                self.parts == other.parts
                and self.pre == other.pre
                and self.post == other.post
            )
        return bool(str(self) == other)

    def __ge__(self, other: Any) -> bool:

        return bool(self == other or self > other)

    def __le__(self, other: Any) -> bool:

        return bool(self == other or self < other)

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
        post=parse_tag_set(post),
    )
