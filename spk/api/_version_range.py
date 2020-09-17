from typing import Optional, Any, Union, Set, TYPE_CHECKING
import abc
from functools import lru_cache
from dataclasses import dataclass, field

from ._compat import Compatibility, COMPATIBLE
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

    def is_applicable(self, other: Version) -> Compatibility:
        """Return true if the given version seems applicable to this range

        Versions that are applicable are not necessarily satisfactory, but
        this cannot be fully determined without a complete package spec.
        """

        gt = self.greater_or_equal_to()
        if gt and not other >= gt:
            return Compatibility("version too low")
        lt = self.less_than()
        if lt and not other < lt:
            return Compatibility("version too high")
        return COMPATIBLE

    @abc.abstractmethod
    def is_satisfied_by(self, spec: "Spec") -> Compatibility:
        """Return true if the given package spec satisfies this version range."""

        pass

    def contains(self, other: "VersionRange") -> Compatibility:

        self_lower = self.greater_or_equal_to()
        self_upper = self.less_than()
        other_lower = other.greater_or_equal_to()
        other_upper = other.less_than()

        if self_lower and other_lower:
            if self_lower > other_lower:
                return Compatibility(
                    f"{other} represents a wider range than allowed by {self}"
                )
        if self_upper and other_upper:
            if self_upper < other_upper:
                return Compatibility(
                    f"{other} represents a wider range than allowed by {self}"
                )

        return self.intersects(other)

    def intersects(self, other: "VersionRange") -> Compatibility:

        self_lower = self.greater_or_equal_to()
        self_upper = self.less_than()
        other_lower = other.greater_or_equal_to()
        other_upper = other.less_than()

        if self_upper and other_lower:
            if self_upper < other_lower:
                return Compatibility(
                    f"{other} does not intersect with {self}, all versions too low"
                )
        if self_lower and other_upper:
            if self_lower > other_upper:
                return Compatibility(
                    f"{other} does not intersect with {self}, all versions too high"
                )

        return COMPATIBLE


class SemverRange(VersionRange):
    def __init__(self, minimum: Union[str, Version]) -> None:

        if not isinstance(minimum, Version):
            minimum = parse_version(minimum)

        self._base = minimum.clone()

    def __str__(self) -> str:

        return f"^{self._base}"

    __repr__ = __str__

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:
        return self._base.clone()

    @lru_cache()
    def less_than(self) -> Optional[Version]:

        parts = list(self._base.parts)
        for i, p in enumerate(parts):
            if p == 0:
                continue
            parts[i] = p + 1
            parts = parts[: i + 1]
            break
        else:
            parts[-1] += 1

        return Version.from_parts(*parts)

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:
        return self.is_applicable(spec.pkg.version)


class WildcardRange(VersionRange):
    def __init__(self, minimum: str) -> None:

        self._specified = len(minimum.split(VERSION_SEP))
        self._parts = minimum.split(VERSION_SEP)
        if self._parts.count("*") != 1:
            raise ValueError("Expected exactly one wildcard in version range: {self}")

    def __str__(self) -> str:

        return f"{VERSION_SEP.join(str(p) for p in self._parts)}"

    __repr__ = __str__

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:

        parts = (str(p) if p != "*" else "0" for p in self._parts)
        v = f"{VERSION_SEP.join(parts)}"
        return parse_version(v)

    @lru_cache()
    def less_than(self) -> Optional[Version]:

        first_wildcard = self._parts.index("*")
        if first_wildcard <= 0:
            return None

        str_parts = list(self._parts[:first_wildcard])
        if not str_parts:
            str_parts = ["0"]
        parts = list(int(i) for i in str_parts)
        parts[-1] += 1
        return Version.from_parts(*parts)

    def is_applicable(self, version: Version) -> Compatibility:

        for i, (a, b) in enumerate(zip(self._parts, version.parts)):

            if a != "*" and int(a) != b:
                return Compatibility(f"Out of range: {self} [at pos {i}]")

        return COMPATIBLE

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:

        return self.is_applicable(spec.pkg.version)


class LowestSpecifiedRange(VersionRange):
    def __init__(self, minimum: str) -> None:

        self._specified = len(minimum.split(VERSION_SEP))
        self._base = parse_version(minimum)

    def __str__(self) -> str:

        parts = list(self._base.parts[: self._specified])

        return f"~{VERSION_SEP.join(str(p) for p in parts)}"

    __repr__ = __str__

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:
        return self._base.clone()

    @lru_cache()
    def less_than(self) -> Optional[Version]:

        parts = list(self._base.parts[: self._specified - 1])
        parts[-1] += 1
        return parse_version(VERSION_SEP.join(str(p) for p in parts))

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:
        return self.is_applicable(spec.pkg.version)


class GreaterThanRange(VersionRange):
    def __init__(self, boundary: Union[str, Version]) -> None:

        if not isinstance(boundary, Version):
            boundary = parse_version(boundary)

        self._bound = boundary

    def __str__(self) -> str:

        return f">{self._bound}"

    __repr__ = __str__

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:
        return self._bound

    @lru_cache()
    def less_than(self) -> Optional[Version]:
        return None

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:

        return self.is_applicable(spec.pkg.version)

    def is_applicable(self, version: Version) -> Compatibility:

        if not version > self._bound:
            return Compatibility(f"Not {self} [too low]")
        return COMPATIBLE


class LessThanRange(VersionRange):
    def __init__(self, boundary: Union[str, Version]) -> None:

        if not isinstance(boundary, Version):
            boundary = parse_version(boundary)

        self._bound = boundary

    def __str__(self) -> str:

        return f"<{self._bound}"

    __repr__ = __str__

    def greater_or_equal_to(self) -> Optional[Version]:
        return None

    def less_than(self) -> Optional[Version]:
        return self._bound

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:

        return self.is_applicable(spec.pkg.version)

    def is_applicable(self, version: Version) -> Compatibility:

        if not version < self._bound:
            return Compatibility(f"Not {self} [too high]")
        return COMPATIBLE


class GreaterThanOrEqualToRange(VersionRange):
    def __init__(self, boundary: Union[str, Version]) -> None:

        if not isinstance(boundary, Version):
            boundary = parse_version(boundary)

        self._bound = boundary

    def __str__(self) -> str:

        return f">={self._bound}"

    __repr__ = __str__

    def greater_or_equal_to(self) -> Optional[Version]:
        return self._bound

    def less_than(self) -> Optional[Version]:
        return None

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:

        return self.is_applicable(spec.pkg.version)

    def is_applicable(self, version: Version) -> Compatibility:

        if not version >= self._bound:
            return Compatibility(f"Not {self} [too low]")
        return COMPATIBLE


class LessThanOrEqualToRange(VersionRange):
    def __init__(self, boundary: Union[str, Version]) -> None:

        if not isinstance(boundary, Version):
            boundary = parse_version(boundary)

        self._bound = boundary

    def __str__(self) -> str:

        return f"<={self._bound}"

    __repr__ = __str__

    def greater_or_equal_to(self) -> Optional[Version]:
        return None

    def less_than(self) -> Optional[Version]:
        return None

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:

        return self.is_applicable(spec.pkg.version)

    def is_applicable(self, version: Version) -> Compatibility:

        if not version <= self._bound:
            return Compatibility(f"Not {self} [too high]")
        return COMPATIBLE


class ExactVersion(VersionRange):
    def __init__(self, version: Union[str, Version]) -> None:

        if not isinstance(version, Version):
            version = parse_version(version)

        self._version = version.clone()

    def __str__(self) -> str:

        return f"={self._version}"

    __repr__ = __str__

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:
        return self._version.clone()

    @lru_cache()
    def less_than(self) -> Optional[Version]:

        parts = list(self._version.parts)
        parts[-1] += 1
        return parse_version(VERSION_SEP.join(str(p) for p in parts))

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:

        if not self._version == spec.pkg.version:
            return Compatibility(f"{spec.pkg.version} !! {self} [not equal]")
        return COMPATIBLE


class ExcludedVersion(VersionRange):
    def __init__(self, exclude: str) -> None:

        self._specified = len(exclude.split(VERSION_SEP))
        self._base = parse_version(exclude)

    def __str__(self) -> str:

        parts = list(self._base.parts[: self._specified])
        return f"!={VERSION_SEP.join(str(p) for p in parts)}"

    __repr__ = __str__

    @lru_cache()
    def greater_or_equal_to(self) -> Optional[Version]:
        return None

    @lru_cache()
    def less_than(self) -> Optional[Version]:
        return None

    def is_applicable(self, version: Version) -> Compatibility:

        if version.parts[: self._specified] == self._base.parts[: self._specified]:
            return Compatibility(f"excluded [{self}]")

        return COMPATIBLE

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:

        return self.is_applicable(spec.pkg.version)


class CompatRange(VersionRange):
    def __init__(self, minimum: Union[str, Version]) -> None:

        if not isinstance(minimum, Version):
            minimum = parse_version(minimum)

        self._base = minimum.clone()

    def __str__(self) -> str:

        return str(self._base)

    __repr__ = __str__

    def greater_or_equal_to(self) -> Optional[Version]:
        return self._base.clone()

    def less_than(self) -> Optional[Version]:

        return None

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:

        if not spec.pkg.build or spec.pkg.build.is_source():
            return spec.compat.is_api_compatible(self._base, spec.pkg.version)

        return spec.compat.is_binary_compatible(self._base, spec.pkg.version)


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
        if not mins:
            return None
        return min(filter(None, mins))

    def is_applicable(self, other: Union[str, Version]) -> Compatibility:
        """Return true if the given version number is applicable to this range.

        Versions that are applicable are not necessarily satisfactory, but
        this cannot be fully determined without a complete package spec.
        """
        if not isinstance(other, Version):
            other = parse_version(other)

        for r in self.rules:
            c = r.is_applicable(other)
            if not c:
                return c

        return COMPATIBLE

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:
        """Return true if the given package spec satisfies this version range."""

        for rule in self.rules:
            c = rule.is_satisfied_by(spec)
            if not c:
                return c

        return COMPATIBLE

    def contains(self, other: VersionRange) -> Compatibility:

        if not isinstance(other, VersionFilter):
            other = VersionFilter({other})

        new_rules = other.rules - self.rules
        for new_rule in new_rules:
            for old_rule in self.rules:
                compat = old_rule.contains(new_rule)
                if not compat:
                    return compat

        return COMPATIBLE

    def intersects(self, other: VersionRange) -> Compatibility:

        if not isinstance(other, VersionFilter):
            other = VersionFilter({other})

        new_rules = other.rules - self.rules
        for new_rule in new_rules:
            for old_rule in self.rules:
                compat = old_rule.intersects(new_rule)
                if not compat:
                    return compat

        return COMPATIBLE

    def restrict(self, other: "VersionFilter") -> None:
        """Reduce this range by another

        This version range will become restricted to the intersection
        of the current version range and the other.
        """

        compat = self.intersects(other)
        if not compat:
            raise ValueError(compat)

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
        elif rule_str.startswith("!="):
            rule = ExcludedVersion(rule_str[2:])
        elif "*" in rule_str:
            rule = WildcardRange(rule_str)
        else:
            rule = CompatRange(rule_str)
        out.rules.add(rule)

    return out
