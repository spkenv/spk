from typing import Dict, List, Any, Union, Optional, Set, TYPE_CHECKING
from dataclasses import dataclass, field
import abc

from ._version import Version, parse_version, VERSION_SEP
from ._build import Build, parse_build
from ._ident import Ident, parse_ident
from ._version_range import parse_version_range, VersionFilter

if TYPE_CHECKING:
    from ._spec import Spec


@dataclass
class RangeIdent:
    """Identitfies a range of package versions and builds."""

    name: str
    version: VersionFilter = field(default_factory=lambda: VersionFilter())
    build: Optional[Build] = None

    def __str__(self) -> str:

        out = self.name
        if self.version.rules:
            out += "/" + str(self.version)
        if self.build:
            out += "/" + self.build.digest
        return out

    def is_applicable(self, pkg: Union[str, Ident]) -> bool:
        """Return true if the given package version is applicable to this range.

        Versions that are applicable are not necessarily satisfactory, but
        this cannot be fully determined without a complete package spec.
        """

        if not isinstance(pkg, Ident):
            pkg = parse_ident(pkg)

        if pkg.name != self.name:
            return False

        if not self.version.is_applicable(pkg.version):
            return False

        if self.build is not None:
            if self.build != pkg.build:
                return False

        return True

    def is_satisfied_by(self, spec: "Spec") -> bool:
        """Return true if the given package spec satisfies this request."""

        if spec.pkg.name != self.name:
            return False

        if not self.version.is_satisfied_by(spec):
            return False

        if self.build is not None:
            if self.build != spec.pkg.build:
                return False
        return True

    def restrict(self, other: "RangeIdent") -> None:

        try:
            self.version.restrict(other.version)
        except ValueError as e:
            raise ValueError(f"{e} [{self.name}]") from None

        if self.build == other.build or self.build is None:
            self.build = other.build
        else:
            raise ValueError(f"Incompatible builds: {self} && {other}")


def parse_ident_range(source: str) -> RangeIdent:
    """Parse a package identifier which specifies a range of versions.

    >>> parse_ident_range("maya/~2020.0")
    RangeIdent(name='maya', version=VersionFilter(...), build=None)
    >>> parse_ident_range("maya/^2020.0")
    RangeIdent(name='maya', version=VersionFilter(...), build=None)
    """

    name, version, build, *other = str(source).split("/") + ["", ""]

    if any(other):
        raise ValueError(f"Too many tokens in identifier: {source}")

    return RangeIdent(
        name=name,
        version=parse_version_range(version),
        build=parse_build(build) if build else None,
    )


@dataclass
class Request:
    """A desired package and set of restrictions."""

    pkg: RangeIdent

    def __hash__(self) -> int:

        return hash(self.pkg.name)

    def __eq__(self, other: Any) -> bool:

        if not isinstance(other, Request):
            return bool(str(self) == other)
        return self.__hash__() == other.__hash__()

    def clone(self) -> "Request":

        return Request.from_dict(self.to_dict())

    def is_version_applicable(self, version: Union[str, Version]) -> bool:
        """Return true if the given version number is applicable to this request.

        This is used a cheap preliminary way to prune package
        versions that are not going to satisfy the request without
        needing to load the whole package spec.
        """

        if not isinstance(version, Version):
            version = parse_version(version)

        return self.pkg.version.is_applicable(version)

    def is_satisfied_by(self, spec: "Spec") -> bool:
        """Return true if the given package spec satisfies this request."""

        if not self.pkg.is_satisfied_by(spec):
            return False

        return True

    def restrict(self, other: "Request") -> None:
        """Reduce the scope of this request to the intersection with another."""

        self.pkg.restrict(other.pkg)

    def to_dict(self) -> Dict[str, Any]:
        """Return a serializable dict copy of this request."""
        return {
            "pkg": str(self.pkg),
        }

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Request":
        """Construct a request from it's dictionary representation."""

        try:
            req = Request(parse_ident_range(data.pop("pkg")))
        except KeyError as e:
            raise ValueError(f"Missing required key in package request: {e}")

        if len(data):
            raise ValueError(
                f"unrecognized fields in package request: {', '.join(data.keys())}"
            )

        return req
