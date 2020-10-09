from typing import Dict, List, Any, Union, Optional, Set, TypeVar, TYPE_CHECKING
from dataclasses import dataclass, field
import abc
import enum
import itertools

from ._name import validate_name
from ._version import Version, parse_version, VERSION_SEP
from ._build import Build, parse_build
from ._ident import Ident, parse_ident
from ._version_range import parse_version_range, VersionFilter, ExactVersion
from ._compat import Compatibility, COMPATIBLE

if TYPE_CHECKING:
    from ._spec import Spec

Self = TypeVar("Self")


@dataclass
class RangeIdent:
    """Identitfies a range of package versions and builds."""

    name: str
    version: VersionFilter = field(default_factory=VersionFilter)
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

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:
        """Return true if the given package spec satisfies this request."""

        if spec.pkg.name != self.name:
            return Compatibility("different package names")

        c = self.version.is_satisfied_by(spec)
        if not c:
            return c

        if self.build is not None:
            if self.build != spec.pkg.build:
                return Compatibility(
                    f"different builds: {self.build} != {spec.pkg.build}"
                )

        return COMPATIBLE

    def contains(self, other: "RangeIdent") -> Compatibility:

        if other.name != self.name:
            return Compatibility(
                f"Version selectors are for different packages: {self.name} != {other.name}"
            )

        compat = self.version.contains(other.version)
        if not compat:
            return compat

        if other.build is None:
            return COMPATIBLE
        elif self.build == other.build or self.build is None:
            return COMPATIBLE
        else:
            return Compatibility(f"Incompatible builds: {self} && {other}")

    def restrict(self, other: "RangeIdent") -> None:

        try:
            self.version.restrict(other.version)
        except ValueError as e:
            raise ValueError(f"{e} [{self.name}]") from None

        if other.build is None:
            pass
        elif self.build == other.build or self.build is None:
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
        name=validate_name(name),
        version=parse_version_range(version),
        build=parse_build(build) if build else None,
    )


class PreReleasePolicy(enum.IntEnum):

    ExcludeAll = enum.auto()
    IncludeAll = enum.auto()


class InclusionPolicy(enum.IntEnum):

    Always = enum.auto()
    IfAlreadyPresent = enum.auto()


class Request(metaclass=abc.ABCMeta):
    """Represents a contraint added to a resolved environment."""

    @abc.abstractproperty
    def name(self) -> str:
        """Return the canonical name of this requirement."""
        pass

    @abc.abstractmethod
    def is_satisfied_by(self, spec: "Spec") -> Compatibility:
        """Return true if the given package spec satisfies this request."""
        pass

    @abc.abstractmethod
    def clone(self: Self) -> Self:
        """Return a copy of this request instance."""
        pass

    @abc.abstractmethod
    def to_dict(self) -> Dict[str, Any]:
        """Return a serializable dict copy of this request."""
        pass

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Request":
        """Construct a request from it's dictionary representation."""

        if "pkg" in data:
            return PkgRequest.from_dict(data)
        if "var" in data:
            return VarRequest.from_dict(data)

        raise ValueError(f"Incomprehensible request definition: {data}")


@dataclass
class VarRequest(Request):
    """A set of restrictions placed on selected packages' build options."""

    var: str
    value: str

    def name(self) -> str:
        """Return the canonical name of this requirement."""
        return self.var

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:
        """Return true if the given package spec satisfies this request."""
        raise NotImplementedError("VarRequest.is_satisfied_by")

    def clone(self) -> "VarRequest":
        """Return a copy of this request instance."""
        return VarRequest.from_dict(self.to_dict())

    def to_dict(self) -> Dict[str, Any]:
        """Return a serializable dict copy of this request."""

        return {"var": f"{self.var}/{self.value}"}

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "VarRequest":

        var = data.pop("var")
        if "/" not in var:
            raise ValueError(f"var request must be in the form name/value, got '{var}'")

        var, value = var.split("/", 1)
        request = VarRequest(var=var, value=value)

        if len(data):
            raise ValueError(
                f"unrecognized fields in var request: {', '.join(data.keys())}"
            )

        return request


@dataclass
class PkgRequest(Request):
    """A desired package and set of restrictions on how it's selected."""

    pkg: RangeIdent
    prerelease_policy: PreReleasePolicy = PreReleasePolicy.ExcludeAll
    inclusion_policy: InclusionPolicy = InclusionPolicy.Always
    pin: str = ""

    def __hash__(self) -> int:

        return hash(self.pkg.name)

    def __eq__(self, other: Any) -> bool:

        if not isinstance(other, Request):
            return bool(str(self) == other)
        return self.__hash__() == other.__hash__()

    @property
    def name(self) -> str:
        return self.pkg.name

    def clone(self) -> "PkgRequest":

        return PkgRequest.from_dict(self.to_dict())

    def render_pin(self, pkg: Ident) -> "PkgRequest":
        """Create a copy of this request with it's pin rendered out using 'pkg'."""

        if not self.pin:
            raise RuntimeError("Request has no pin to be rendered")

        digits = itertools.chain(pkg.version.parts, itertools.repeat(0))
        rendered = list(self.pin)
        for i, char in enumerate(self.pin):
            if char == "x":
                rendered[i] = str(next(digits))

        new = self.clone()
        new.pin = ""
        new.pkg.version = parse_version_range("".join(rendered))
        return new

    def is_version_applicable(self, version: Union[str, Version]) -> Compatibility:
        """Return true if the given version number is applicable to this request.

        This is used a cheap preliminary way to prune package
        versions that are not going to satisfy the request without
        needing to load the whole package spec.
        """

        if not isinstance(version, Version):
            version = parse_version(version)

        if self.prerelease_policy is PreReleasePolicy.ExcludeAll and version.pre:
            return Compatibility("prereleases not allowed")

        return self.pkg.version.is_applicable(version)

    def is_satisfied_by(self, spec: "Spec") -> Compatibility:
        """Return true if the given package spec satisfies this request."""

        if spec.deprecated:
            # deprecated builds are only okay if their build
            # was specifically requested
            if self.pkg.build is not None and self.pkg.build == spec.pkg.build:
                pass
            else:
                return Compatibility(
                    "Build is deprecated and was not specifically requested"
                )

        if (
            self.prerelease_policy is PreReleasePolicy.ExcludeAll
            and spec.pkg.version.pre
        ):
            return Compatibility("prereleases not allowed")

        return self.pkg.is_satisfied_by(spec)

    def restrict(self, other: "PkgRequest") -> None:
        """Reduce the scope of this request to the intersection with another."""

        self.prerelease_policy = PreReleasePolicy(
            min(self.prerelease_policy.value, other.prerelease_policy.value)
        )
        self.inclusion_policy = InclusionPolicy(
            min(self.inclusion_policy.value, other.inclusion_policy.value)
        )
        self.pkg.restrict(other.pkg)

    def to_dict(self) -> Dict[str, Any]:
        """Return a serializable dict copy of this request."""
        out = {"pkg": str(self.pkg), "prereleasePolicy": self.prerelease_policy.name}
        if self.inclusion_policy is not InclusionPolicy.Always:
            out["include"] = self.inclusion_policy.name
        if self.pin:
            out["fromBuildEnv"] = self.pin
        return out

    @staticmethod
    def from_ident(pkg: Ident) -> "Request":

        ri = RangeIdent(
            name=pkg.name,
            version=VersionFilter({ExactVersion(pkg.version),}),
            build=pkg.build,
        )
        return PkgRequest(ri)

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "PkgRequest":
        """Construct a request from it's dictionary representation."""

        try:
            req = PkgRequest(parse_ident_range(data.pop("pkg")))
        except KeyError as e:
            raise ValueError(f"Missing required key in package request: {e}")

        if "prereleasePolicy" in data:
            name = data.pop("prereleasePolicy")
            try:
                policy = PreReleasePolicy.__members__[name]
            except KeyError:
                raise ValueError(
                    f"Unknown 'prereleasePolicy': {name} must be on of {list(PreReleasePolicy.__members__.keys())}"
                )
            req.prerelease_policy = policy

        inclusion_policy = data.pop("include", InclusionPolicy.Always.name)
        try:
            req.inclusion_policy = InclusionPolicy.__members__[inclusion_policy]
        except KeyError:
            raise ValueError(
                f"Unknown 'include' policy: {inclusion_policy} must be on of {list(InclusionPolicy.__members__.keys())}"
            )

        req.pin = data.pop("fromBuildEnv", "")
        if req.pin and req.pkg.version.rules:
            raise ValueError(
                "Package request cannot include both a version number and fromBuildEnv"
            )

        if len(data):
            raise ValueError(
                f"unrecognized fields in package request: {', '.join(data.keys())}"
            )

        return req
