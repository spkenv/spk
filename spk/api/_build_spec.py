from typing import Dict, List, Any, Union, Set
import os
import abc
from dataclasses import dataclass, field

from ._request import Request, parse_ident_range
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
    def validate(self, value: str) -> Compatibility:
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

    def resolve_all_options(self, given: OptionMap) -> OptionMap:
        resolved = OptionMap()
        for opt in self.options:

            name = opt.name()
            given_value = given.get(name, "")
            value = opt.get_value(given_value)
            resolved[name] = value

        return resolved

    def validate_options(self, given_options: OptionMap) -> Compatibility:
        """Validate the given options against the options in this spec."""

        for option in self.options:
            compat = option.validate(given_options.get(option.name(), ""))
            if not compat:
                return compat

        return COMPATIBLE

    def upsert_opt(self, opt: Union[str, Request, Option]) -> None:
        """Add or update an option in this build spec.

        An option is replaced if it shares a name with the given option,
        otherwise the option is appended to the buid options
        """
        if isinstance(opt, str):
            opt = Request(parse_ident_range(opt))
        if isinstance(opt, Request):
            opt = opt_from_request(opt)
        for i, other in enumerate(self.options):
            if other.name() == opt.name():
                self.options[i] = opt
                break
        else:
            self.options.append(opt)

    def to_dict(self) -> Dict[str, Any]:
        spec: Dict[str, Any] = {
            "options": list(o.to_dict() for o in self.options),
        }
        if self.script != BuildSpec().script:
            spec["script"] = self.script.splitlines()
        if self.variants != BuildSpec().variants:
            spec["variants"] = list(dict(v) for v in self.variants)
        return spec

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "BuildSpec":
        """Construct a BuildSpec from a dictionary config."""

        bs = BuildSpec()
        if "script" in data:
            script = data.pop("script")
            if isinstance(script, list):
                script = "\n".join(script)
            bs.script = script

        options = data.pop("options", [])
        if options:
            bs.options = list(opt_from_dict(opt) for opt in options)

        variants = data.pop("variants", [])
        if variants:
            bs.variants = list(OptionMap.from_dict(v) for v in variants)

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

    raise ValueError("Incomprehensible option definition")


def opt_from_request(request: Request) -> "PkgOpt":
    """Create a build option from the given request."""

    return PkgOpt(
        pkg=request.pkg.name, default=str(request.pkg)[len(request.pkg.name) + 1 :]
    )


@dataclass
class VarOpt(Option):

    var: str
    default: str = ""
    choices: Set[str] = field(default_factory=set)

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

    def validate(self, value: str) -> Compatibility:

        if not value:
            return COMPATIBLE

        assigned = super(VarOpt, self).get_value()
        if assigned:
            if assigned == value:
                return COMPATIBLE
            return Compatibility(
                f"Incompatible option: wanted '{value}', got '{assigned}'"
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
        return spec

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "VarOpt":

        try:
            var = data.pop("var")
        except KeyError:
            raise ValueError("missing required key for VarOpt: var")

        opt = VarOpt(var)
        opt.default = data.pop("default", "")
        opt.choices = set(str(c) for c in data.pop("choices", []))
        opt.set_value(data.pop("static", ""))

        if len(data):
            raise ValueError(f"unrecognized fields in var: {', '.join(data.keys())}")

        return opt


@dataclass
class PkgOpt(Option):

    pkg: str
    default: str = ""

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

    def validate(self, value: str) -> Compatibility:

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
        return Request(pkg=parse_ident_range(f"{self.pkg}/{value}"))

    def to_dict(self) -> Dict[str, Any]:

        spec = {"pkg": self.pkg}
        if self.default:
            spec["default"] = self.default
        base_value = super(PkgOpt, self).get_value()
        if base_value:
            spec["static"] = base_value
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
        opt.set_value(data.pop("static", ""))

        if len(data):
            raise ValueError(f"unrecognized fields in pkg: {', '.join(data.keys())}")

        return opt
