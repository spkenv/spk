from typing import Dict, List, Any, Optional, Tuple, Union, Set
import os
import abc
import enum
from dataclasses import dataclass, field

from ._request import Request, PkgRequest, parse_ident_range, PreReleasePolicy
from ._option_map import OptionMap
from ._name import validate_name
from ._compat import Compatibility, COMPATIBLE


class Option(metaclass=abc.ABCMeta):
    def __init__(self) -> None:
        self.__value: str = ""

    @abc.abstractmethod
    def name(self) -> str:
        pass

    @abc.abstractmethod
    def validate(self, value: Optional[str]) -> Compatibility:
        pass

    @abc.abstractmethod
    def to_dict(self) -> Dict[str, Any]:
        pass

    def set_value(self, value: str) -> None:
        """Assign a value to this option.

        Once a value is assigned, it overrides any 'given' value on future access.
        """

        self.__value = value

    def get_value(self, given: str = None) -> str:
        """Return the current value of this option, if set.

        Given is only returned if the option is not currently set to something else.
        """

        if self.__value:
            return self.__value

        return given or ""


@dataclass
class BuildSpec:
    """A set of structured inputs used to build a package."""

    script: str = "sh ./build.sh"
    options: List[Option] = field(default_factory=list)
    variants: List[OptionMap] = field(default_factory=lambda: [OptionMap()])

    def resolve_all_options(
        self, package_name: str, given: Union[Dict, OptionMap] = {}
    ) -> OptionMap:

        if not isinstance(given, OptionMap):
            given = OptionMap(given.items())

        resolved = OptionMap()
        given = given.package_options(package_name)
        for opt in self.options:

            name = opt.name()
            given_value = given.get(name, None)
            value = opt.get_value(given_value)
            resolved[name] = value

        return resolved

    def validate_options(
        self, package_name: str, given_options: Union[Dict, OptionMap]
    ) -> Compatibility:
        """Validate the given options against the options in this spec."""

        if not isinstance(given_options, OptionMap):
            given_options = OptionMap(given_options.items())

        must_exist = given_options.package_options_without_global(package_name)
        given_options = given_options.package_options(package_name)
        for option in self.options:
            compat = option.validate(given_options.get(option.name()))
            if not compat:
                return compat

            try:
                del must_exist[option.name()]
            except KeyError:
                pass

        missing = list(must_exist.keys())
        if missing:
            missing = list(name for name in missing)
            return Compatibility(
                f"Package does not define requested build options: {missing}"
            )

        return COMPATIBLE

    def upsert_opt(self, opt: Union[str, Request, Option]) -> None:
        """Add or update an option in this build spec.

        An option is replaced if it shares a name with the given option,
        otherwise the option is appended to the buid options
        """
        if isinstance(opt, str):
            opt = PkgRequest(parse_ident_range(opt))
        if isinstance(opt, Request):
            opt = opt_from_request(opt)
        for i, other in enumerate(self.options):
            if other.name() == opt.name():
                self.options[i] = opt
                break
        else:
            self.options.append(opt)

    def to_dict(self) -> Dict[str, Any]:
        spec: Dict[str, Any] = {}
        if self.options:
            spec["options"] = list(o.to_dict() for o in self.options)
        if self.script != BuildSpec().script:
            spec["script"] = self.script.splitlines()
        if self.variants != BuildSpec().variants:
            spec["variants"] = list(dict(v) for v in self.variants)
        return spec

    @staticmethod
    def from_dict_unsafe(data: Dict[str, Any]) -> "BuildSpec":
        """Construct a BuildSpec from a dictionary config without checking validation rules."""
        bs = BuildSpec()
        if "script" in data:
            script = data.get("script", "")
            if isinstance(script, list):
                script = "\n".join(script)
            bs.script = script

        options = data.get("options", [])
        if options:
            bs.options = list(opt_from_dict(opt) for opt in options)

        variants = data.get("variants", [])
        if variants:
            bs.variants = list(OptionMap.from_dict(v) for v in variants)
        return bs

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "BuildSpec":
        """Construct a BuildSpec from a dictionary config."""

        bs = BuildSpec.from_dict_unsafe(data)
        data.pop("script", None)
        data.pop("options", None)
        variants = data.pop("variants", [])

        unique_options = set()
        for opt in bs.options:
            if opt.name() in unique_options:
                raise ValueError(f"Build option specified more than once: {opt.name()}")
            unique_options.add(opt.name())

        variant_builds: List[Tuple[str, OptionMap]] = []
        unique_variants = set()
        for variant in variants:
            build_opts = bs.resolve_all_options("", variant)
            digest = build_opts.digest()
            variant_builds.append((digest, variant))
            unique_variants.add(digest)
        if len(unique_variants) < len(variant_builds):
            raise ValueError(
                "Multiple variants would produce the same build:\n"
                + "\n".join(f"- {o} ({h})" for (h, o) in variant_builds)
            )

        if len(data):
            raise ValueError(
                f"unrecognized fields in spec.build: {', '.join(data.keys())}"
            )

        return bs


def opt_from_dict(data: Dict[str, Any]) -> Option:

    if "pkg" in data:
        return PkgOpt.from_dict(data)
    if "var" in data:
        return VarOpt.from_dict(data)

    raise ValueError(f"Incomprehensible option definition: {data}")


def opt_from_request(request: Request) -> "PkgOpt":
    """Create a build option from the given request."""

    if isinstance(request, PkgRequest):
        return PkgOpt(
            pkg=request.pkg.name,
            default=str(request.pkg)[len(request.pkg.name) + 1 :],
            prerelease_policy=request.prerelease_policy,
        )

    raise ValueError(f"Cannot convert {type(request)} to option")


class Inheritance(enum.Enum):
    """Defines the way in which a build option in inherited by downstream packages."""

    weak = "Weak"
    strong = "Strong"


class VarOpt(Option):
    def __init__(self, var: str, default: str = "", choices: Set[str] = None) -> None:
        self.var = var
        self.default = default
        self.choices = choices if choices else set()
        self.inheritance = Inheritance.weak
        super(VarOpt, self).__init__()

    def __repr__(self) -> str:
        return f"VarOpt({self.to_dict()})"

    def name(self) -> str:
        return self.var

    def get_value(self, given: str = None) -> str:

        assigned = super(VarOpt, self).get_value(given)
        if assigned:
            return assigned

        if given is not None:
            return given

        return self.default

    def set_value(self, value: str) -> None:

        if value and self.choices and value not in self.choices:
            raise ValueError(
                f"Invalid value '{value}' for option '{self.var}', must be one of {self.choices}"
            )
        super(VarOpt, self).set_value(value)

    def validate(self, value: Optional[str]) -> Compatibility:

        if value is None:
            value = self.default

        assigned = super(VarOpt, self).get_value()
        if assigned:
            if not value or assigned == value:
                return COMPATIBLE
            return Compatibility(
                f"Incompatible option '{self.var}': wanted '{value}', got '{assigned}'"
            )

        if self.choices and value not in self.choices:
            return Compatibility(
                f"Invalid value '{value}' for option '{self.var}', must be one of {self.choices}"
            )

        return COMPATIBLE

    def to_dict(self) -> Dict[str, Any]:

        spec: Dict[str, Any] = {"var": self.var}
        if self.default:
            spec["default"] = self.default

        if self.choices:
            spec["choices"] = list(self.choices)

        base_value = super(VarOpt, self).get_value()
        if base_value:
            spec["static"] = base_value

        if self.inheritance is not Inheritance.weak:
            spec["inheritance"] = self.inheritance.value

        return spec

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "VarOpt":

        try:
            var = data.pop("var")
        except KeyError:
            raise ValueError("missing required key for VarOpt: var")

        opt = VarOpt(var)
        opt.default = str(data.pop("default", ""))
        opt.choices = set(str(c) for c in data.pop("choices", []))
        opt.set_value(str(data.pop("static", "")))

        inheritance = str(data.pop("inheritance", Inheritance.weak.value))
        opt.inheritance = Inheritance(inheritance)

        if len(data):
            raise ValueError(f"unrecognized fields in var: {', '.join(data.keys())}")

        return opt


class PkgOpt(Option):
    def __init__(
        self,
        pkg: str,
        default: str = "",
        prerelease_policy: PreReleasePolicy = PreReleasePolicy.ExcludeAll,
    ) -> None:
        self.pkg = pkg
        self.default = default
        self.prerelease_policy = prerelease_policy
        super(PkgOpt, self).__init__()

    def __repr__(self) -> str:
        return f"PkgOpt({self.to_dict()})"

    def name(self) -> str:
        return self.pkg

    def get_value(self, given: str = None) -> str:

        assigned = super(PkgOpt, self).get_value(given)
        if assigned:
            return assigned

        if given is not None:
            return given

        return self.default

    def set_value(self, value: str) -> None:

        try:
            parse_ident_range(f"{self.pkg}/{value}")
        except ValueError as err:
            raise ValueError(
                f"Invalid value '{value}' for option '{self.pkg}', not a valid package request: {err}"
            )
        super(PkgOpt, self).set_value(value)

    def validate(self, value: Optional[str]) -> Compatibility:

        if value is None:
            value = ""

        # skip any default that might exist since
        # that does not represent a definitive range
        base = super(PkgOpt, self).get_value()
        base_range = parse_ident_range(f"{self.pkg}/{base}")
        try:
            value_range = parse_ident_range(f"{self.pkg}/{value}")
        except ValueError as err:
            return Compatibility(
                f"Invalid value '{value}' for option '{self.pkg}', not a valid package request: {err}"
            )

        return value_range.contains(base_range)

    def to_request(self, given_value: str = None) -> Request:

        value = self.default
        if given_value is not None:
            value = given_value
        return PkgRequest(
            pkg=parse_ident_range(f"{self.pkg}/{value}"),
            prerelease_policy=self.prerelease_policy,
        )

    def to_dict(self) -> Dict[str, Any]:

        spec = {"pkg": self.pkg}
        if self.default:
            spec["default"] = self.default
        base_value = super(PkgOpt, self).get_value()
        if base_value:
            spec["static"] = base_value
        if self.prerelease_policy is not PreReleasePolicy.ExcludeAll:
            spec["prereleasePolicy"] = self.prerelease_policy.name
        return spec

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "PkgOpt":

        try:
            pkg = data.pop("pkg")
        except KeyError:
            raise ValueError("missing required key for PkgOpt: pkg")

        if "/" in pkg:
            raise ValueError(
                "Build option for package cannot have version number, use 'default' field instead"
            )
        pkg = validate_name(pkg)

        default = str(data.pop("default", ""))
        opt = PkgOpt(pkg, default=default)

        if "prereleasePolicy" in data:
            name = data.pop("prereleasePolicy")
            try:
                policy = PreReleasePolicy.__members__[name]
            except KeyError:
                raise ValueError(
                    f"Unknown 'prereleasePolicy': {name} must be on of {list(PreReleasePolicy.__members__.keys())}"
                )
            opt.prerelease_policy = policy

        opt.set_value(str(data.pop("static", "")))

        if len(data):
            raise ValueError(f"unrecognized fields in pkg: {', '.join(data.keys())}")

        return opt
