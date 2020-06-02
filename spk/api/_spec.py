from typing import List, Any, Dict, Union, IO
from dataclasses import dataclass, field
import os

import structlog
from ruamel import yaml


from ._ident import Ident, parse_ident
from ._request import Request
from ._option_map import OptionMap
from ._build_spec import BuildSpec
from ._source_spec import SourceSpec, LocalSource


_LOGGER = structlog.get_logger("spk")


@dataclass
class Spec:
    """Spec encompases the complete specification of a package."""

    pkg: "Ident"
    sources: List[SourceSpec] = field(default_factory=list)
    build: BuildSpec = field(default_factory=BuildSpec)
    opts: List[Union["Request", "VarSpec"]] = field(default_factory=list)
    depends: List["Request"] = field(default_factory=list)
    provides: List["Spec"] = field(default_factory=list)

    def resolve_all_options(self, given: OptionMap) -> OptionMap:

        resolved = OptionMap()
        for opt in self.opts:

            if isinstance(opt, Spec):
                name = opt.pkg.name

            elif isinstance(opt, VarSpec):
                name = opt.var

            else:
                raise NotImplementedError(f"Unhandled option type: {type(opt)}")

            env_var = f"SPM_OPT_{name}"
            if env_var in os.environ:
                src = "environ"
                value = os.environ[env_var]

            elif name in given:
                src = "given"
                value = given[name]

            # TODO: get a default value from definition

            else:
                src = "none"
                value = ""

            resolved[name] = value

        return resolved

    def sastisfies_request(self, request: Request) -> bool:
        """Return true if this package spec satisfies the given request."""

        return False

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Spec":

        pkg = parse_ident(data.pop("pkg", ""))
        spec = Spec(pkg)
        spec.build = BuildSpec.from_dict(data.pop("build", {}))
        for src in data.pop("sources", [{"path": "."}]):
            spec.sources.append(SourceSpec.from_dict(src))
        for opt in data.pop("opts", []):
            spec.opts.append(opt_from_dict(opt))
        for dep in data.pop("depends", []):
            spec.depends.append(Request.from_dict(dep))
        for provided in data.pop("provides", []):
            spec.provides.append(Spec.from_dict(provided))

        if len(data):
            raise ValueError(f"unrecognized fields in spec: {', '.join(data.keys())}")

        return spec

    def to_dict(self) -> Dict[str, Any]:

        return {
            "pkg": self.pkg,
            "build": self.build.to_dict(),
            "opts": list(o.to_dict() for o in self.opts),
            "depends": list(d.to_dict() for d in self.depends),
            "provides": list(p.to_dict() for p in self.provides),
        }


def read_spec_file(filepath: str) -> Spec:
    """ReadSpec loads a package specification from a yaml file."""

    filepath = os.path.abspath(filepath)
    with open(filepath, "r") as f:
        spec = read_spec(f)

    spec_root = os.path.dirname(filepath)
    for source in spec.sources:
        if isinstance(source, LocalSource):
            source.path = os.path.join(spec_root, source.path)

    return spec


def read_spec(stream: IO[str]) -> Spec:

    yaml_data = yaml.safe_load(stream)
    return Spec.from_dict(yaml_data)


def write_spec(spec: Spec) -> bytes:

    return yaml.dump(spec.to_dict()).encode()  # type: ignore


def opt_from_dict(data: Dict[str, Any]) -> Union[Request, "VarSpec"]:

    if "pkg" in data:
        return Request.from_dict(data)
    if "var" in data:
        return VarSpec.from_dict(data)

    raise ValueError("Incomprehensible option definition")


@dataclass
class VarSpec:

    var: str

    def to_dict(self) -> Dict[str, Any]:

        return {"var": self.var}

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "VarSpec":

        try:
            var = data["var"]
        except KeyError:
            raise ValueError("missing required key for VarSpec: var")

        return VarSpec(var)
