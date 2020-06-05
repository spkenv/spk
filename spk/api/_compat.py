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

    def check(self, base: Version, other: Version) -> bool:
        """Return true if the two version are compatible by this compat rule."""

        for rule, a, b in zip(self.parts, base.parts, other.parts):

            if rule == COMPAT_NONE:
                if a != b:
                    return False
            # FIXME: handle binary compat better
            elif rule == COMPAT_API or rule == COMPAT_ABI:
                if a > b:
                    return False
            else:
                raise NotImplementedError("Unhandled compat specifier: " + rule)

        return True


def parse_compat(compat: str) -> Compat:
    """Parse a string as a compatibility specifier."""

    return Compat(compat)
