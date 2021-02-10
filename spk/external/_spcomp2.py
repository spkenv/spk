from os import openpty
from typing import Iterable, List, Iterable, Optional, Tuple, Dict
import os
import re
from pathlib import Path
from packaging.markers import Value

from ruamel import yaml

import structlog

from .. import api, storage, build


_LOGGER = structlog.get_logger("spk.external.spcomp2")
SPCOMP2_ROOT = "/shots/spi/home/lib/SpComp2"
SPCOMP2_EXCLUDED_BUILDS = (
    "do-not-use",
    "lion",
    "copy",
    ".rhel",
    "darwin",
    "windows",
    "c++03",
    "-ins",
    "linux-",
)
CURRENT = "current"
BUILD_SCRIPT = """\
cd {root}/{name}/{build}/{version}
libs=(lib/*.so.*)
rsync -rav --usermap=$(id -u):$(id -g) --links --exclude '.svn' "${{libs[@]}}" /spfs/lib/
rsync -rav --usermap=$(id -u):$(id -g) --links --exclude 'v*' --exclude '.svn' include/ /spfs/include/
for lib in "${{libs[@]}}"; do
    chmod +w /spfs/$lib
    chrpath --delete $lib
done
"""


def import_spcomp2(
    name: str,
    spcomp2_version: str = CURRENT,
    options: api.OptionMap = api.host_options(),
    install_root: str = SPCOMP2_ROOT,
    recursive: bool = True,
) -> List[api.Spec]:
    """Import an SpComp2 into the spk ecosystem.

    Args:
      name (str): the name of the spComp2 to import
      version (str): the version of the spComp2 to import
      options (spk.api.OptionMap): import all spComp2s compatible with these build options
        (defaults to the current host machine)
      install_root (str): root spComp2 location to discover spComp2 packages
      recursive (bool): if true, also import all required dependencies

    Returns:
      List(spk.api.Spec): The imported packages, which will exist in the local repo
    """

    _LOGGER.info("importing spcomp2...", name=name, version=spcomp2_version)

    imported = []
    root = Path(install_root)
    for build_str in iter_spcomp2_builds(name):
        build_spec = api.BuildSpec(options=_build_to_options(build_str))
        build_opts = build_spec.resolve_all_options(name, options)
        compat = build_spec.validate_options(name, build_opts)
        if not compat:
            _LOGGER.debug(
                "skipping incompatible build", build=build_str, error=str(compat)
            )
            continue

        build_dir = root.joinpath(name, build_str)
        version_dir = build_dir.joinpath(spcomp2_version).resolve()
        version_cfg = version_dir.joinpath("version.cfg")

        try:
            with version_cfg.open("r") as reader:
                version_config = yaml.safe_load(reader) or {}
        except FileNotFoundError:
            continue

        spk_name = _to_spk_name(name)
        latest = _get_latest_patch_version(version_dir)
        if latest is None:
            continue
        spec = api.Spec(
            pkg=api.Ident(
                spk_name,
                latest,
                build=api.Build(build_opts.digest()),
            ),
            compat=api.parse_compat("x.ab"),
            build=build_spec,
        )

        spec.build.script = BUILD_SCRIPT.format(
            root=str(root.absolute()),
            name=name,
            build=build_str,
            version=version_dir.name,
        )

        _LOGGER.info("scanning dependencies...")
        spcomp2_depends = version_config.get("spcomp2_depend", "")
        for dep in spcomp2_depends.split(" "):
            if not dep:
                continue
            dep_name, dep_version = dep.split("?", 1)
            assert dep_version.startswith(
                "v="
            ), f"Failed to parse spComp2 dependency: {dep_version}"
            dep_version = dep_version.replace("=", "", 1)

            if recursive:
                imported.extend(
                    import_spcomp2(dep_name, dep_version, options, install_root)
                )

            version_range = api.parse_ident_range(
                f"{_to_spk_name(dep_name)}/{dep_version.lstrip('v')}"
            )
            spec.install.requirements.append(api.PkgRequest(version_range))

        # stdfs is required for the general include/lib configuration
        spec.install.requirements.append(
            api.PkgRequest(pkg=api.parse_ident_range("stdfs"))
        )

        _LOGGER.info("building", pkg=spec.pkg)
        spec = (
            build.BinaryPackageBuilder.from_spec(spec)
            .with_options(build_opts)
            .with_source(".")
            .with_repository(storage.local_repository())
            .with_repository(storage.remote_repository())
            .build()
        )
        imported.append(spec)
    return imported


def iter_spcomp2_builds(name: str, install_root: str = SPCOMP2_ROOT) -> Iterable[str]:
    """Iterate the available build for the named spComp2."""

    root = Path(install_root).joinpath(name)
    for build in os.listdir(root):
        if not root.joinpath(build).is_dir():
            continue
        if "-" not in build:
            continue
        for excl in SPCOMP2_EXCLUDED_BUILDS:
            if excl in build:
                break
        else:
            yield build


def _get_latest_patch_version(version_dir: Path) -> Optional[api.Version]:
    """Use the spComp2 so files to determine the highest patch version.

    Args:
      version_dir (pathlib.Path): The spComp2 major version dir
    """

    lib_dir = version_dir.joinpath("lib")
    so_files = lib_dir.glob("*.so.*")
    versions = []
    for filepath in so_files:
        _, version_str = filepath.name.split(".so.")
        versions.append(api.parse_version(version_str))
    if not versions:
        return None
    return sorted(versions)[-1]


def _to_spk_name(name: str) -> str:

    return name.lower().replace("_", "-")


KNOWN_OPTIONS: List[Tuple[re.Pattern, str, str, List[api.Option]]] = [
    (
        re.compile(r"rhel(7)"),
        "var",
        "centos",
        [
            api.opt_from_dict({"var": "os", "choices": ["linux"]}),
            api.opt_from_dict({"var": "distro", "choices": ["centos"]}),
        ],
    ),
    (
        re.compile(r"rhel(\d+)"),
        "var",
        "rhel",
        [
            api.opt_from_dict({"var": "os", "choices": ["linux"]}),
            api.opt_from_dict({"var": "distro", "choices": ["rhel"]}),
        ],
    ),
    (
        re.compile(r"gcc(\d)(\d+)"),
        "pkg",
        "gcc",
        [api.opt_from_dict({"var": "arch", "choices": ["x86_64"]})],
    ),
    (
        re.compile(r"spinux(\d)"),
        "var",
        "spinux",
        [
            api.opt_from_dict({"var": "os", "choices": ["linux"]}),
            api.opt_from_dict({"var": "distro", "choices": ["spinux"]}),
        ],
    ),
    (re.compile(r"boost(\d)_?(\d+)"), "pkg", "boost", []),
    (re.compile(r"ice(\d)(\d)"), "pkg", "ice", []),
    (re.compile(r"py(\d)(\d)"), "pkg", "python", []),
]


def _build_to_options(build: str) -> List[api.Option]:

    # stdfs is used to configure the base unix-style directory structure
    options = [api.opt_from_dict({"pkg": "stdfs"})]
    for option in build.split("-"):
        for pattern, kind, name, static in KNOWN_OPTIONS:
            match = pattern.match(option)
            if not match:
                continue
            options.append(
                api.opt_from_dict({kind: name, "static": ".".join(match.groups())})
            )
            for opt in static:
                options.append(opt)
            break
        else:
            raise ValueError(f"Unhandled spcomp2 build option: {option}")
    return options
