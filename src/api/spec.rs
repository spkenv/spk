// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use serde::{Deserialize, Serialize};

use super::{
    parse_compat, parse_ident, parse_version_range, request::is_false, BuildSpec, Compat,
    Compatibility, Ident, Inheritance, LocalSource, Opt, OptionMap, PkgOpt, PkgRequest, RangeIdent,
    Request, SourceSpec, TestSpec, VarOpt, VarRequest,
};
use crate::{Error, Result};

#[macro_export]
macro_rules! spec {
    ($($k:ident => $v:expr),* $(,)?) => {{
        use std::convert::TryInto;
        let mut spec = Spec::default();
        $(spec.$k = $v.try_into().unwrap();)*
        spec
    }};
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    pub pkg: Ident,
    #[serde(default, skip_serializing_if = "Compat::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceSpec>,
    pub build: BuildSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<TestSpec>,
    pub install: InstallSpec,
}

/// A set of structured installation parameters for a package.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct InstallSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    requirements: Vec<Request>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    embedded: Vec<Spec>,
}

impl InstallSpec {
    pub fn is_empty(&self) -> bool {
        self.requirements.is_empty() && self.embedded.is_empty()
    }

    /// Add or update a requirement to the set of installation requirements.
    ///
    /// If a request exists for the same name, it is replaced with the given
    /// one. Otherwise the new request is appended to the list.
    pub fn upsert_requirement(&mut self, request: Request) {
        let name = request.name();
        for other in self.requirements.iter_mut() {
            if other.name() == name {
                std::mem::replace(other, request);
                return;
            }
        }
        self.requirements.push(request);
    }

    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<'a>(
        &mut self,
        options: &OptionMap,
        resolved: impl Iterator<Item = &'a Ident>,
    ) -> Result<()> {
        let mut by_name = std::collections::HashMap::new();
        for pkg in resolved {
            by_name.insert(pkg.name(), pkg);
        }
        for request in self.requirements.iter_mut() {
            match request {
                Request::Pkg(request) => {
                    if request.pin.is_none() {
                        continue;
                    }
                    match by_name.get(&request.pkg.name()) {
                        None => {
                            return Err(Error::String(
                                format!("Cannot resolve fromBuildEnv, package not present: {}\nIs it missing from your package build options?", request.pkg.name())
                            ));
                        }
                        Some(resolved) => {
                            std::mem::replace(request, request.render_pin(resolved)?);
                        }
                    }
                }
                Request::Var(request) => {
                    if !request.pin {
                        continue;
                    }
                    let split = request.var.splitn(2, ".");
                    let var = split.last().unwrap();
                    let opts = match split.next() {
                        Some(package) => options.package_options(package),
                        None => options.clone(),
                    };
                    match opts.get(var) {
                        None => {
                            return Err(Error::String(
                                format!("Cannot resolve fromBuildEnv, variable not set: {}\nIs it missing from the package build options?", request.var)
                            ));
                        }
                        Some(opt) => {
                            std::mem::replace(request, request.render_pin(opt)?);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for InstallSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Unchecked {
            #[serde(default)]
            requirements: Vec<Request>,
            #[serde(default)]
            embedded: Vec<Spec>,
        }

        let unchecked = Unchecked::deserialize(deserializer)?;
        let spec = InstallSpec {
            requirements: unchecked.requirements,
            embedded: unchecked.embedded,
        };

        let requirement_names = std::collections::HashSet::with_capacity(spec.requirements.len());
        for name in spec.requirements.iter().map(Request::name) {
            if requirement_names.contains(&name) {
                return Err(serde::de::Error::custom(format!(
                    "found multiple install requirements for '{}'",
                    name
                )));
            }
            requirement_names.insert(name);
        }

        let mut default_build_spec = BuildSpec::default();
        for embedded in spec.embedded.iter() {
            default_build_spec.options = embedded.build.options.clone();
            if default_build_spec != embedded.build {
                return Err(serde::de::Error::custom(
                    "embedded packages can only specify build.options",
                ));
            }
            if !embedded.install.is_empty() {
                return Err(serde::de::Error::custom(
                    "embedded packages cannot specify the install field",
                ));
            }
            if let Some(_) = embedded.pkg.build {
                return Err(serde::de::Error::custom(format!(
                    "embedded package should not specify a build, got: {}",
                    embedded.pkg
                )));
            }
        }

        Ok(spec)
    }
}

/*
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
