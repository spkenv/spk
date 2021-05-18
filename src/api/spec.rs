// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::{Compat, Ident};

#[macro_export]
macro_rules! spec {
    ($($k:ident => $v:expr),* $(,)?) => {{
        use std::convert::TryInto;
        let mut spec = Spec::default();
        $(spec.$k = $v.try_into().unwrap();)*
        spec
    }};
}

#[derive(Debug, Default, Clone)]
pub struct Spec {
    pub pkg: Ident,
    pub compat: Compat,
    pub deprecated: bool,
}

/*
from ast import parse
from typing import List, Any, Dict, Optional, Union, IO, Iterable
from dataclasses import dataclass, field
import os

import structlog
from ruamel import yaml

from ._build import EMBEDDED
from ._ident import Ident, parse_ident
from ._compat import Compat, Compatibility, COMPATIBLE, parse_compat
from ._request import Request, PkgRequest, VarRequest, RangeIdent, parse_version_range
from ._option_map import OptionMap
from ._build_spec import BuildSpec, PkgOpt, VarOpt, Inheritance, Option
from ._test_spec import TestSpec
from ._source_spec import SourceSpec, LocalSource


_LOGGER = structlog.get_logger("spk")


@dataclass
class InstallSpec:
    """A set of structured installation parameters for a package."""

    requirements: List[Request] = field(default_factory=list)
    embedded: List["Spec"] = field(default_factory=list)

    def upsert_requirement(self, request: Request) -> None:
        """Add or update a requirement to the set of installation requirements.

        If a request exists for the same name, it is replaced with the given
        one. Otherwise the new request is appended to the list.
        """
        for i, other in enumerate(self.requirements):
            if other.name == request.name:
                self.requirements[i] = request
                return
        else:
            self.requirements.append(request)

    def to_dict(self) -> Dict[str, Any]:
        data = {}
        if self.requirements:
            data["requirements"] = list(r.to_dict() for r in self.requirements)
        if self.embedded:
            data["embedded"] = list(r.to_dict() for r in self.embedded)
        return data

    def render_all_pins(self, options: OptionMap, resolved: Iterable[Ident]) -> None:
        """Render all requests with a package pin using the given resolved packages."""

        by_name = dict((pkg.name, pkg) for pkg in resolved)
        for i, request in enumerate(self.requirements):

            if isinstance(request, PkgRequest):
                if not request.pin:
                    continue
                if request.pkg.name not in by_name:
                    raise ValueError(
                        f"Cannot resolve fromBuildEnv, package not present: {request.pkg.name}\n"
                        "Is it missing from your package build options?"
                    )
                self.requirements[i] = request.render_pin(by_name[request.pkg.name])

            elif isinstance(request, VarRequest):
                if not request.pin:
                    continue
                var = request.var
                opts = options
                if "." in var:
                    package, var = var.split(".", 1)
                    opts = options.package_options(package)
                if var not in opts:
                    raise ValueError(
                        f"Cannot resolve fromBuildEnv, variable not set: {request.var}\n"
                        "Is it missing from the package build options?"
                    )
                self.requirements[i] = request.render_pin(opts[var])

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "InstallSpec":

        spec = InstallSpec()

        requirements = data.pop("requirements", [])
        assert isinstance(requirements, list), "install.requirements must be a list"
        if requirements:
            spec.requirements = list(Request.from_dict(r) for r in requirements)
        request_names = list(r.name for r in spec.requirements)
        while request_names:
            name = request_names.pop()
            if name in request_names:
                raise ValueError(f"found multiple install requirements for '{name}'")

        embedded = data.pop(
            "embedded", data.pop("embeded", [])  # legacy support of misspelling
        )
        assert isinstance(requirements, list), "install.embedded must be a list"
        for e in embedded:
            if "build" in e:
                if tuple(e["build"].keys()) != ("options",):
                    raise ValueError("embedded packages can only specify build.options")
            if "install" in e:
                raise ValueError("embedded packages cannot specify the install field")
            es = Spec.from_dict(e)
            if es.pkg.build is not None and not es.pkg.build.is_emdeded():
                raise ValueError(
                    f"embedded package should not specify a build, got: {es.pkg}"
                )
            for opt in es.build.options:
                opt.to_dict
            es.pkg.set_build(EMBEDDED)
            spec.embedded.append(es)

        if len(data):
            raise ValueError(
                f"unrecognized fields in spec.install: {', '.join(data.keys())}"
            )

        return spec


@dataclass
class Spec:
    """Spec encompases the complete specification of a package."""

    pkg: Ident = field(default_factory=Ident)
    compat: Compat = field(default_factory=Compat)
    deprecated: bool = False
    sources: List[SourceSpec] = field(default_factory=list)
    build: BuildSpec = field(default_factory=BuildSpec)
    tests: List[TestSpec] = field(default_factory=list)
    install: InstallSpec = field(default_factory=InstallSpec)

    def __hash__(self) -> int:
        return hash(self.pkg)

    def clone(self) -> "Spec":
        return Spec.from_dict(self.to_dict())

    def resolve_all_options(self, given: Union[OptionMap, Dict[str, Any]]) -> OptionMap:
        """Return the full set of resolved build options using the given ones."""

        if not isinstance(given, OptionMap):
            given = OptionMap(given)

        return self.build.resolve_all_options(self.pkg.name, given)

    def sastisfies_request(self, request: Request) -> Compatibility:
        """Check if this package spec satisfies the given request."""

        if isinstance(request, PkgRequest):
            return self.satisfies_pkg_request(request)
        elif isinstance(request, VarRequest):
            return self.satisfies_var_request(request)
        else:
            raise NotImplementedError(f"Unhandled request type: {type(request)}")

    def satisfies_var_request(self, request: VarRequest) -> Compatibility:
        """Check if this package spec satisfies the given var request."""

        opt_required = request.package() == self.pkg.name
        opt: Optional[Option] = None
        for o in self.build.options:
            if request.name() in (o.name(), o.namespaced_name(self.pkg.name)):
                opt = o
                break

        if opt is None:
            if opt_required:
                return Compatibility(
                    f"Package does not define requested option: {request.var}"
                )
            return COMPATIBLE

        if isinstance(opt, PkgOpt):
            return opt.validate(request.value)

        if not isinstance(opt, VarOpt):
            _LOGGER.warning(f"Unhandled option type: {type(opt)}")
            return COMPATIBLE

        exact = opt.get_value(request.value)
        if exact != request.value:
            return Compatibility(
                f"Incompatible build option '{request.var}': '{exact}' != '{request.value}'"
            )

        return COMPATIBLE

    def satisfies_pkg_request(self, request: PkgRequest) -> Compatibility:
        """Check if this package spec satisfies the given pkg request."""

        if request.pkg.name != self.pkg.name:
            return Compatibility(
                f"different package name: {request.pkg.name} != {self.pkg.name}"
            )

        compat = request.is_satisfied_by(self)
        if not compat:
            return compat

        if request.pkg.build is None:
            return COMPATIBLE

        if request.pkg.build == self.pkg.build:
            return COMPATIBLE

        return Compatibility(
            f"Package and request differ in builds: requested {request.pkg.build}, got {self.pkg.build}"
        )

    def update_for_build(self, options: OptionMap, resolved: Iterable["Spec"]) -> None:
        """Update this spec to represent a specific binary package build."""

        specs = dict((s.pkg.name, s) for s in resolved)
        for dep_name, dep_spec in specs.items():
            for opt in dep_spec.build.options:
                if not isinstance(opt, VarOpt):
                    continue
                if opt.inheritance is Inheritance.weak:
                    continue
                inherited_opt = VarOpt.from_dict(opt.to_dict())
                if "." not in inherited_opt.var:
                    inherited_opt.var = f"{dep_name}.{opt.var}"
                inherited_opt.inheritance = Inheritance.weak
                self.build.upsert_opt(inherited_opt)
                if opt.inheritance is Inheritance.strong:
                    req = VarRequest(inherited_opt.var, pin=True)
                    self.install.upsert_requirement(req)

        build_options = list(self.build.options)
        for e in self.install.embedded:
            build_options.extend(e.build.options)

        for opt in build_options:
            if not isinstance(opt, PkgOpt):
                opt.set_value(options.get(opt.name(), opt.get_value()))
                continue

            spec = specs.get(opt.pkg)
            if spec is None:
                raise ValueError("PkgOpt missing in resolved: " + opt.pkg)

            opt.set_value(str(spec.compat.render(spec.pkg.version)))

        self.install.render_all_pins(options, (spec.pkg for spec in resolved))
        self.pkg.set_build(self.resolve_all_options(options).digest())

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "Spec":

        pkg = parse_ident(data.pop("pkg", ""))
        spec = Spec(pkg)
        if "compat" in data:
            spec.compat = parse_compat(data.pop("compat"))
        if "deprecated" in data:
            spec.deprecated = bool(data.pop("deprecated"))
        for src in data.pop("sources", [{"path": "."}]):
            spec.sources.append(SourceSpec.from_dict(src))
        for test in data.pop("tests", []):
            spec.tests.append(TestSpec.from_dict(test))
        if pkg.build is not None:
            # if the build is set, we assume that this is a rendered spec
            # and we do not want to make an existing rendered build spec unloadable
            spec.build = BuildSpec.from_dict_unsafe(data.pop("build", {}))
        else:
            spec.build = BuildSpec.from_dict(data.pop("build", {}))
        spec.install = InstallSpec.from_dict(data.pop("install", {}))

        if len(data):
            raise ValueError(f"unrecognized fields in spec: {', '.join(data.keys())}")

        return spec

    def to_dict(self) -> Dict[str, Any]:

        spec: Dict[str, Any] = {}
        if self.pkg != Ident(""):
            spec["pkg"] = str(self.pkg)
        if self.compat != Compat():
            spec["compat"] = str(self.compat)
        if self.deprecated:
            spec["deprecated"] = self.deprecated

        if self.sources:
            spec["sources"] = [src.to_dict() for src in self.sources]
        if self.tests:
            spec["tests"] = [test.to_dict() for test in self.tests]

        build = self.build.to_dict()
        if build:
            spec["build"] = build
        install = self.install.to_dict()
        if install:
            spec["install"] = install
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
            original_data = yaml.round_trip_load(reader) or {}
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

    yaml_data = yaml.safe_load(stream) or {}
    return Spec.from_dict(yaml_data)


def write_spec(spec: Spec) -> bytes:

    return yaml.dump(spec.to_dict()).encode()  # type: ignore
*/
