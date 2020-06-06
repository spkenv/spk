from typing import Optional, Any, Union, Set, TYPE_CHECKING
import abc
from functools import lru_cache
from dataclasses import dataclass, field

from ._version import Version, parse_version, VERSION_SEP

if TYPE_CHECKING:
    from ._spec import Spec

VERSION_RANGE_SEP = ","


class VersionRange(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def greater_or_equal_to(self) -> Optional[Version]:
        pass

    @abc.abstractmethod
    def less_than(self) -> Optional[Version]:
        pass

    @abc.abstractmethod
    def __str__(self) -> str:
        pass

    def __hash__(self) -> int:
        return hash(self.__str__())

    def __eq__(self, other: Any) -> bool:

        return str(self) == str(other)

    def is_applicable(self, other: Version) -> bool:
        """Return true if the given version seems applicable to this range

        Versions that are applicable are not necessarily satisfactory, but
        this cannot be fully determined without a complete package spec.
        """

        gt = self.greater_or_equal_to()
        if gt and not other >= gt:
            return False
        lt = self.less_than()
        if lt and not other < lt:
            return False
        return True

    @abc.abstractmethod
    def is_satisfied_by(self, spec: "Spec") -> bool:
        """Return true if the given package spec satisfies this version range."""

        pass


class SemverRange(VersionRange):
    def __init__(self, minimum: Union[str, Version]) -> None:

        if not isinstance(minimum, Version):
            minimum = parse_version(minimum)

        self._base = minimum.clone()

    def __str__(self) -> str:

        return f"^{self._base}"

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:
        return self._base.clone()

    @lru_cache()
    def less_than(self) -> Optional[Version]:

        return Version(self._base.major + 1)

    def is_satisfied_by(self, spec: "Spec") -> bool:
        return self.is_applicable(spec.pkg.version)


class WildcardRange(VersionRange):
    def __init__(self, minimum: str) -> None:

        self._specified = len(minimum.split(VERSION_SEP))
        self._parts = tuple(
            int(p) if p != "*" else p for p in minimum.split(VERSION_SEP)
        )

    def __str__(self) -> str:

        return f"{VERSION_SEP.join(str(p) for p in self._parts)}"

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:

        parts = (str(p) if p != "*" else "0" for p in self._parts)
        v = f"{VERSION_SEP.join(parts)}"
        return parse_version(v)

    @lru_cache()
    def less_than(self) -> Optional[Version]:

        first_wildcard = self._parts.index("*")
        if first_wildcard < 0:
            return None

        parts = list(self._parts[:first_wildcard])
        parts[-1] += 1  # type: ignore
        return Version(*parts[:3], parts[3:])  # type: ignore

    def is_applicable(self, version: Version) -> bool:

        for a, b in zip(self._parts, version.parts):

            if a != b and a != "*":
                return False

        return True

    def is_satisfied_by(self, spec: "Spec") -> bool:

        return self.is_applicable(spec.pkg.version)


class LowestSpecifiedRange(VersionRange):
    def __init__(self, minimum: str) -> None:

        self._specified = len(minimum.split(VERSION_SEP))
        self._base = parse_version(minimum)

    def __str__(self) -> str:

        parts = list(self._base.parts[: self._specified])

        return f"~{VERSION_SEP.join(str(p) for p in parts)}"

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:
        return self._base.clone()

    @lru_cache()
    def less_than(self) -> Optional[Version]:

        parts = list(self._base.parts[: self._specified - 1])
        parts[-1] += 1
        return parse_version(VERSION_SEP.join(str(p) for p in parts))

    def is_satisfied_by(self, spec: "Spec") -> bool:
        return self.is_applicable(spec.pkg.version)


class GreaterThanRange(VersionRange):
    def __init__(self, boundary: Union[str, Version]) -> None:

        if not isinstance(boundary, Version):
            boundary = parse_version(boundary)

        self._bound = boundary

    def __str__(self) -> str:

        return f">{self._bound}"

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:
        return self._bound

    @lru_cache()
    def less_than(self) -> Optional[Version]:
        return None

    def is_satisfied_by(self, spec: "Spec") -> bool:

        return self.is_applicable(spec.pkg.version)

    def is_applicable(self, version: Version) -> bool:

        return version > self._bound


class LessThanRange(VersionRange):
    def __init__(self, boundary: Union[str, Version]) -> None:

        if not isinstance(boundary, Version):
            boundary = parse_version(boundary)

        self._bound = boundary

    def __str__(self) -> str:

        return f"<{self._bound}"

    def greater_or_equal_to(self) -> Optional[Version]:
        return None

    def less_than(self) -> Optional[Version]:
        return self._bound

    def is_satisfied_by(self, spec: "Spec") -> bool:

        return self.is_applicable(spec.pkg.version)

    def is_applicable(self, version: Version) -> bool:

        return version < self._bound


class GreaterThanOrEqualToRange(VersionRange):
    def __init__(self, boundary: Union[str, Version]) -> None:

        if not isinstance(boundary, Version):
            boundary = parse_version(boundary)

        self._bound = boundary

    def __str__(self) -> str:

        return f">={self._bound}"

    def greater_or_equal_to(self) -> Optional[Version]:
        return self._bound

    def less_than(self) -> Optional[Version]:
        return None

    def is_satisfied_by(self, spec: "Spec") -> bool:

        return self.is_applicable(spec.pkg.version)

    def is_applicable(self, version: Version) -> bool:

        return version >= self._bound


class LessThanOrEqualToRange(VersionRange):
    def __init__(self, boundary: Union[str, Version]) -> None:

        if not isinstance(boundary, Version):
            boundary = parse_version(boundary)

        self._bound = boundary

    def __str__(self) -> str:

        return f"<={self._bound}"

    def greater_or_equal_to(self) -> Optional[Version]:
        return None

    def less_than(self) -> Optional[Version]:
        return None

    def is_satisfied_by(self, spec: "Spec") -> bool:

        return self.is_applicable(spec.pkg.version)

    def is_applicable(self, version: Version) -> bool:

        return version <= self._bound


class ExactVersion(VersionRange):
    def __init__(self, version: Union[str, Version]) -> None:

        if not isinstance(version, Version):
            version = parse_version(version)

        self._version = version.clone()

    def __str__(self) -> str:

        return f"={self._version}"

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:
        return self._version.clone()

    @lru_cache()
    def less_than(self) -> Optional[Version]:

        parts = list(self._version.parts)
        parts[-1] += 1
        return parse_version(VERSION_SEP.join(str(p) for p in parts))

    def is_satisfied_by(self, spec: "Spec") -> bool:

        return self._version == spec.pkg.version


class CompatRange(VersionRange):
    def __init__(self, minimum: Union[str, Version]) -> None:

        if not isinstance(minimum, Version):
            minimum = parse_version(minimum)

        self._base = minimum.clone()

    def __str__(self) -> str:

        return str(self._base)

    def greater_or_equal_to(self) -> Optional[Version]:
        return self._base.clone()

    def less_than(self) -> Optional[Version]:

        return None

    def is_satisfied_by(self, spec: "Spec") -> bool:

        return spec.compat.check(self._base, spec.pkg.version)


@dataclass
class VersionFilter(VersionRange):

    rules: Set[VersionRange] = field(default_factory=set)

    def __str__(self) -> str:
        return VERSION_RANGE_SEP.join(list(str(r) for r in self.rules))

    def greater_or_equal_to(self) -> Optional[Version]:

        mins = list(filter(None, (v.greater_or_equal_to() for v in self.rules)))
        if not mins:
            return None
        return min(mins)

    def less_than(self) -> Optional[Version]:

        mins = list(v.less_than() for v in self.rules)
        return min(filter(None, mins))

    def is_applicable(self, other: Union[str, Version]) -> bool:
        """Return true if the given version number is applicable to this range.

        Versions that are applicable are not necessarily satisfactory, but
        this cannot be fully determined without a complete package spec.
        """
        if not isinstance(other, Version):
            other = parse_version(other)

        return all(r.is_applicable(other) for r in self.rules)

    def is_satisfied_by(self, spec: "Spec") -> bool:
        """Return true if the given package spec satisfies this version range."""

        for rule in self.rules:
            if not rule.is_satisfied_by(spec):
                return False
        return True

    def restrict(self, other: "VersionFilter") -> None:
        """Reduce this range by another

        This version range will become restricted to the intersection
        of the current version range and the other.
        """

        self.rules |= other.rules


def parse_version_range(range: str) -> VersionFilter:

    rules = range.split(VERSION_RANGE_SEP)
    if not range:
        rules = []
    out = VersionFilter()

    for rule_str in rules:
        rule: VersionRange
        if not rule_str:
            raise ValueError("Empty segment not allowed in version range")
        elif rule_str.startswith("^"):
            rule = SemverRange(rule_str[1:])
        elif rule_str.startswith("~"):
            rule = LowestSpecifiedRange(rule_str[1:])
        elif rule_str.startswith(">="):
            rule = GreaterThanOrEqualToRange(rule_str[2:])
        elif rule_str.startswith("<="):
            rule = LessThanOrEqualToRange(rule_str[2:])
        elif rule_str.startswith(">"):
            rule = GreaterThanRange(rule_str[1:])
        elif rule_str.startswith("<"):
            rule = LessThanRange(rule_str[1:])
        elif rule_str.startswith("="):
            rule = ExactVersion(rule_str[1:])
        elif "*" in rule_str:
            rule = WildcardRange(rule_str)
        else:
            rule = CompatRange(rule_str)
        out.rules.add(rule)

    return out
