from typing import Union, Tuple, Any
from dataclasses import dataclass
import enum


from ._version import VERSION_SEP, Version

COMPAT_NONE = "x"
COMPAT_API = "a"
COMPAT_ABI = "b"


class Compat:
    """Compat specifies the compatilbility contract of a compat number."""

    def __init__(self, spec: str = "x.a.b") -> None:

        if spec:
            self.parts = tuple(spec.split(VERSION_SEP))
        else:
            self.parts = tuple()

    def __str__(self) -> str:

        return str(VERSION_SEP.join(self.parts))

    __repr__ = __str__

    def __eq__(self, other: Any) -> bool:

        if isinstance(other, Compat):
            return self.parts == other.parts
        return bool(str(self) == other)

    def clone(self) -> "Compat":

        return Compat(VERSION_SEP.join(self.parts))

    def is_api_compatible(self, base: Version, other: Version) -> bool:
        """Return true if the two version are api compatible by this compat rule."""

        return self._check_compat(base, other, COMPAT_API)

    def is_binary_compatible(self, base: Version, other: Version) -> bool:
        """Return true if the two version are binary compatible by this compat rule."""

        return self._check_compat(base, other, COMPAT_ABI)

    def _check_compat(self, base: Version, other: Version, required: str) -> bool:

        for rule, a, b in zip(self.parts, base.parts, other.parts):

            if required in rule:
                if b < a:
                    return False
                return True
            if a != b:
                return False

        return True


def parse_compat(compat: str) -> Compat:
    """Parse a string as a compatibility specifier."""

    return Compat(compat)
