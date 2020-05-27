from typing import Union, Tuple, Any
from dataclasses import dataclass

VERSION_SEP = "."


class Version:
    """Version specifies a package version number."""

    def __init__(self, spec: str) -> None:

        if spec:
            self.parts = tuple(spec.split(VERSION_SEP))
        else:
            self.parts = tuple()

    def __str__(self) -> str:

        return str(VERSION_SEP.join(self.parts))

    __repr__ = __str__

    def __eq__(self, other: Any) -> bool:

        if isinstance(other, Version):
            return self.parts == other.parts
        return bool(str(self) == other)

    def copy(self) -> "Version":

        return Version(str(self))

    def is_satisfied_by(self, other: Union[str, "Version"]) -> bool:

        if not isinstance(other, Version):
            other = Version(other)

        # TODO: resolve better and handle ranges
        return self.parts == other.parts[: len(self.parts)]

    def restrict(self, other: "Version") -> None:

        if other.is_satisfied_by(self):
            return

        if not self.is_satisfied_by(other):
            raise ValueError(f"Cannot restict: {other} is not a subset of {self}")

        print(self.parts, other.parts)
        self.parts = other.parts


def parse_version(version: str) -> Version:
    """Parse a string as a version specifier."""

    return Version(version)
