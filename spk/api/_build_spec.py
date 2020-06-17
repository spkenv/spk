from typing import Dict, List, Any, Union
import os
from dataclasses import dataclass, field

from ._request import Request
from ._option_map import OptionMap

Option = Union[Request, "VarSpec"]


@dataclass
class BuildSpec:
    """A set of structured inputs to build a package."""

    script: str = "sh ./build.sh"
    options: List[Option] = field(default_factory=list)
    variants: List[OptionMap] = field(default_factory=lambda: [OptionMap()])

    def resolve_all_options(self, given: OptionMap) -> OptionMap:
        resolved = OptionMap()
        for opt in self.options:

            if isinstance(opt, Request):
                name = opt.pkg.name
                default = str(opt.pkg.version)

            elif isinstance(opt, VarSpec):
                name = opt.var
                default = opt.default

            else:
                raise NotImplementedError(f"Unhandled option type: {type(opt)}")

            env_var = f"SPM_OPT_{name}"
            if env_var in os.environ:
                value = os.environ[env_var]

            elif name in given:
                value = given[name]

            else:
                value = default

            resolved[name] = value

        return resolved

    def to_dict(self) -> Dict[str, Any]:
        return {
            "options": list(o.to_dict() for o in self.options),
            "script": self.script.splitlines(),
            "variants": list(dict(v) for v in self.variants),
        }

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "BuildSpec":

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


def opt_from_dict(data: Dict[str, Any]) -> Union[Request, "VarSpec"]:

    if "pkg" in data:
        return Request.from_dict(data)
    if "var" in data:
        return VarSpec.from_dict(data)

    raise ValueError("Incomprehensible option definition")


@dataclass
class VarSpec:

    var: str
    default: str = ""

    def to_dict(self) -> Dict[str, Any]:

        return {"var": self.var, "default": self.default}

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "VarSpec":

        try:
            var = data.pop("var")
        except KeyError:
            raise ValueError("missing required key for VarSpec: var")

        default = data.pop("default", "")

        if len(data):
            raise ValueError(f"unrecognized fields in var: {', '.join(data.keys())}")

        return VarSpec(var, default=default)
