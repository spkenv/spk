from typing import Union
from dataclasses import dataclass


@dataclass
class Version:
    """Version specifies a package version number."""

    source: str

    def __str__(self) -> str:

        return str(self.source)

    def is_satisfied_by(self, other: Union[str, "Version"]) -> bool:

        if not isinstance(other, Version):
            other = Version(other)

        # TODO: resolve better and handle ranges
        return other.source.startswith(self.source)


def parse_version(version: str) -> Version:
    """Parse a string as a version specifier."""

    return Version(version)
