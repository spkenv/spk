from typing import List, Any, Dict, Union, IO, Iterable
from dataclasses import dataclass, field
import os

import structlog
from ruamel import yaml


from ._ident import Ident, parse_ident
from ._compat import Compat, parse_compat
from ._request import Request
from ._option_map import OptionMap
from ._build_spec import BuildSpec, PkgOpt
from ._install_spec import InstallSpec
from ._source_spec import SourceSpec, LocalSource


_LOGGER = structlog.get_logger("spk")


@dataclass
class Spec:
    """Spec encompases the complete specification of a package."""

    pkg: Ident
    compat: Compat = field(default_factory=Compat)
    sources: List[SourceSpec] = field(default_factory=list)
    build: BuildSpec = field(default_factory=BuildSpec)
    install: InstallSpec = field(default_factory=InstallSpec)

    def clone(self) -> "Spec":
        return Spec.from_dict(self.to_dict())

    def resolve_all_options(self, given: Union[OptionMap, Dict[str, Any]]) -> OptionMap:

        if not isinstance(given, OptionMap):
            given = OptionMap(given)

        given = given.package_options(self.pkg.name)
        return self.build.resolve_all_options(given)

    def sastisfies_request(self, request: Request) -> bool:
        """Return true if this package spec satisfies the given request."""

        if request.pkg.name != self.pkg.name:
            return False

        if not request.is_satisfied_by(self):
            return False

        if request.pkg.build is None:
            return True

        return request.pkg.build == self.pkg.build

    def update_for_build(self, options: OptionMap, resolved: List["Spec"]) -> None:
        """Update this spec to represent a specific binary package build."""

        self.install.render_all_pins(s.pkg for s in resolved)

        specs = dict((s.pkg.name, s) for s in resolved)
        for opt in self.build.options:
            if not isinstance(opt, PkgOpt):
                opt.set_value(options.get(opt.name(), ""))
                continue

            spec = specs.get(opt.pkg)
            if spec is None:
                raise ValueError("PkgOpt missing in resolved: " + opt.pkg)

            opt.set_value(str(spec.compat.render(spec.pkg.version)))

        self.pkg.set_build(self.resolve_all_options(options).digest())

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Spec":

        pkg = parse_ident(data.pop("pkg", ""))
        spec = Spec(pkg)
        if "compat" in data:
            spec.compat = parse_compat(data.pop("compat"))
        for src in data.pop("sources", [{"path": "."}]):
            spec.sources.append(SourceSpec.from_dict(src))
        spec.build = BuildSpec.from_dict(data.pop("build", {}))
        spec.install = InstallSpec.from_dict(data.pop("install", {}))

        if len(data):
            raise ValueError(f"unrecognized fields in spec: {', '.join(data.keys())}")

        return spec

    def to_dict(self) -> Dict[str, Any]:

        spec: Dict[str, Any] = {
            "build": self.build.to_dict(),
            "install": self.install.to_dict(),
        }
        if self.compat != Compat():
            spec["compat"] = str(self.compat)
        if self.pkg != Ident(""):
            spec["pkg"] = str(self.pkg)
        return spec


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


def save_spec_file(filepath: str, spec: Spec) -> None:
    """Save the given spec to a file.

    If the named file already exists, update the spec while trying
    to maintain formatting and comments.
    """

    try:
        with open(filepath, "r") as reader:
            original_data = yaml.round_trip_load(reader)
    except (FileNotFoundError, yaml.YAMLError):
        original_data = {}

    new_data = spec.to_dict()
    _update_dict(original_data, new_data)
    with open(filepath, "w+") as writer:
        yaml.round_trip_dump(original_data, writer)


def _update_dict(original_data: Dict[str, Any], new_data: Dict[str, Any]) -> None:

    for name, data in new_data.items():
        if name not in original_data:
            original_data[name] = data
            continue
        if isinstance(data, dict):
            _update_dict(original_data[name], data)
        if isinstance(data, list):
            _update_list(original_data[name], data)
        else:
            original_data[name] = data
    for name in list(original_data.keys()):
        if name not in new_data:
            del original_data[name]


def _update_list(original_data: List[Any], new_data: List[Any]) -> None:

    for i, data in enumerate(new_data):
        if i >= len(original_data):
            original_data.append(data)
            continue
        if isinstance(data, dict):
            _update_dict(original_data[i], data)
        if isinstance(data, list):
            _update_list(original_data[i], data)
    while len(original_data) > len(new_data):
        original_data.pop(len(new_data))


def read_spec(stream: IO[str]) -> Spec:

    yaml_data = yaml.safe_load(stream)
    return Spec.from_dict(yaml_data)


def write_spec(spec: Spec) -> bytes:

    return yaml.dump(spec.to_dict()).encode()  # type: ignore
