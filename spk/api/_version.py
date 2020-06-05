from typing import Union, Tuple, Any
from dataclasses import dataclass, field

VERSION_SEP = "."


@dataclass
class Version:
    """Version specifies a package version number."""

    major: int = 0
    minor: int = 0
    patch: int = 0
    tail: Tuple[int, ...] = tuple()

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

    str_parts = version.split(VERSION_SEP)
    parts = tuple(int(p) for p in str_parts)
    return Version(*parts[:3], tail=parts[3:])  # type: ignore
