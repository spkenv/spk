from typing import Union, Tuple, Any, Optional
from dataclasses import dataclass
import enum


from ._version import VERSION_SEP, Version


class CompatRule(enum.Enum):

    NONE = "x"

    # The current logic requires that there is an order to these
    # enums. For example API is less than ABI because it's considered
    # a subset - aka you cannot provide binary compatibility and not
    # API compatibility
    API = "a"
    ABI = "b"


class Compatibility(str):
    """Denotes whether or not something is compatible.

    If not compatible, each instance contains a description
    of the incompatibility. Compatibility instances will properly
    evaluate as a boolean (aka empty string (no issues) == true)
    """

    def __bool__(self) -> bool:
        """Things are truthy/compatible when no error is specified."""
        return len(self) == 0


COMPATIBLE = Compatibility()


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

    def is_api_compatible(self, base: Version, other: Version) -> Compatibility:
        """Return true if the two version are api compatible by this compat rule."""

        return self._check_compat(base, other, CompatRule.API)

    def is_binary_compatible(self, base: Version, other: Version) -> Compatibility:
        """Return true if the two version are binary compatible by this compat rule."""

        return self._check_compat(base, other, CompatRule.ABI)

    def render(self, version: Version) -> str:

        parts = version.parts[: len(self.parts)]
        return f"~{VERSION_SEP.join(str(i) for i in parts)}"

    def _check_compat(
        self, base: Version, other: Version, required: CompatRule
    ) -> Compatibility:

        if base == other:
            return COMPATIBLE

        each = list(zip(self.parts, base.parts, other.parts))
        for i, (rule, a, b) in enumerate(each):

            for char in rule:

                if CompatRule.NONE.value == char:
                    if a != b:
                        return Compatibility(
                            f"Not compatible with {base} [{self} at pos {i}]"
                        )
                    continue

                if char <= required.value and b < a:
                    return Compatibility(
                        f"Not {required.name} compatible with {base} [{self} at pos {i}]"
                    )
                if char >= required.value:
                    return COMPATIBLE

        return Compatibility(
            f"Not compatible: {base} ({self}) [{required.name} compatibility not specified]"
        )


def parse_compat(compat: str) -> Compat:
    """Parse a string as a compatibility specifier."""

    return Compat(compat)
