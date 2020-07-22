from typing import Dict, List, Any, Union
import os
import abc
from dataclasses import dataclass, field

from ._request import Request, parse_ident_range
from ._option_map import OptionMap
from ._name import validate_name

Option = Union["PkgOpt", "VarOpt"]


@dataclass
class BuildSpec:
    """A set of structured inputs to build a package."""

    script: str = "sh ./build.sh"
    options: List[Option] = field(default_factory=list)
    variants: List[OptionMap] = field(default_factory=lambda: [OptionMap()])

    def resolve_all_options(self, given: OptionMap) -> OptionMap:
        resolved = OptionMap()
        for opt in self.options:

            name = opt.name()
            env_var = f"SPM_OPT_{name}"
            if env_var in os.environ:
                value = os.environ[env_var]

            elif name in given:
                value = given[name]

            else:
                value = opt.default

            resolved[name] = value

        return resolved

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


def opt_from_dict(data: Dict[str, Any]) -> Union["PkgOpt", "VarOpt"]:

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
class VarOpt:

    var: str
    default: str = ""

    def name(self) -> str:
        return self.var

    def to_dict(self) -> Dict[str, Any]:

        spec = {"var": self.var}
        if self.default:
            spec["default"] = self.default
        return spec

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "VarOpt":

        try:
            var = data.pop("var")
        except KeyError:
            raise ValueError("missing required key for VarOpt: var")

        default = data.pop("default", "")

        if len(data):
            raise ValueError(f"unrecognized fields in var: {', '.join(data.keys())}")

        return VarOpt(var, default=default)


@dataclass
class PkgOpt:

    pkg: str
    default: str = ""

    def name(self) -> str:
        return self.pkg

    def to_request(self, given_value: str = None) -> Request:

        value = self.default
        if given_value is not None:
            value = given_value
        return Request(pkg=parse_ident_range(f"{self.pkg}/{value}"))

    def to_dict(self) -> Dict[str, Any]:

        spec = {"pkg": self.pkg}
        if self.default:
            spec["default"] = self.default
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

        if len(data):
            raise ValueError(f"unrecognized fields in pkg: {', '.join(data.keys())}")

        return PkgOpt(pkg, default=default)
