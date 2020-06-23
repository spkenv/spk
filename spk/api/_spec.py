from typing import List, Any, Dict, Union, IO
from dataclasses import dataclass, field
import os

import structlog
from ruamel import yaml


from ._ident import Ident, parse_ident
from ._compat import Compat, parse_compat
from ._request import Request
from ._option_map import OptionMap
from ._build_spec import BuildSpec
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

    def resolve_all_options(self, given: OptionMap) -> OptionMap:

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

        return {
            "pkg": str(self.pkg),
            "compat": str(self.compat),
            "build": self.build.to_dict(),
            "install": self.install.to_dict(),
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
