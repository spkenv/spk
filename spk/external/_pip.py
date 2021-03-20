from typing import List, Optional, Set
import re
import tempfile
import subprocess
import pkginfo
import packaging.version
import packaging.markers
from pathlib import Path

import structlog

from .. import api, storage, build, solve, io


_LOGGER = structlog.get_logger("spk.external.pip")
BAKED_PYTHON_PACKAGES = ("setuptools", "pip", "wheel")


def import_pip(
    name: str,
    version: str = "",
    python_version: str = "3",
    python_abi: str = None,
    recursive: bool = True,
) -> List[api.Spec]:
    """Import an SpComp2 into the spk ecosystem.

    Args:
      name (str): the name of the pip to import
      version (str): the version of the pip package to import
      python_version (str): the version of python to import for
      python_abi (str): the exact python abi to import for
      recursive (bool): if true, also import all required dependencies

    Returns:
      List(spk.api.Spec): The imported packages, which will exist in the local repo
    """

    return (
        PipImporter()
        .with_python_version(python_version)
        .with_python_abi(python_abi)
        .recursive(recursive)
        .import_package(name, version)
    )


class PipImporter:
    def __init__(self) -> None:

        self._python_version = "3.7"
        self._python_abi: Optional[str] = None
        self._follow_deps = True
        self._visited: Set[str] = set()

    def with_python_version(self, version: str) -> "PipImporter":

        assert (
            re.match(r"\d+.\d+", version) is not None
        ), "python version must be in the form x.x"
        self._python_version = version
        return self

    def with_python_abi(self, version: Optional[str]) -> "PipImporter":

        self._python_abi = version
        return self

    def recursive(self, recursive: bool) -> "PipImporter":

        self._follow_deps = recursive
        return self

    def import_package(self, name: str, version: str = "") -> List[api.Spec]:

        if name in self._visited:
            _LOGGER.debug("found recursive dependency", name=name)
            return []
        self._visited.add(name)

        _LOGGER.info("fetching pip package...", name=name, version=version)

        converted = []
        with tempfile.TemporaryDirectory() as _tmpdir:

            tmpdir = Path(_tmpdir)
            env_command = ["spk", "env"]
            if self._python_abi is not None:
                env_command += ["-o", "python.abi=" + self._python_abi]
            env_command += [f"python/{self._python_version}"]
            pip_command = [
                "pip",
                "download",
                f"{name}{version}",
                "--no-deps",
                "--dest",
                _tmpdir,
            ]

            _LOGGER.debug(" ".join([*env_command, "--", *pip_command]))
            try:
                subprocess.check_output([*env_command, "--", *pip_command])
            except subprocess.CalledProcessError:
                _LOGGER.error(f"failed to download pip package")

            downloaded = list(tmpdir.glob(f"*"))
            assert (
                len(downloaded) == 1
            ), f"Expected pip to download one file for {name} {downloaded}"

            converted.extend(self.process_pip_package(downloaded[0]))

        return converted

    def process_pip_package(self, filepath: Path) -> List[api.Spec]:

        if filepath.name.endswith(".whl"):
            info = pkginfo.Wheel(filepath)
        elif filepath.name.endswith(".tar.gz"):
            info = pkginfo.SDist(filepath)
        else:
            raise NotImplementedError(
                f"No logic to install distribution format: {filepath}"
            )
        return self._process_package(info)

    def _process_package(self, info: pkginfo.Distribution) -> List[api.Spec]:

        assert not info.requires, "No support for installation requirements"
        assert not info.requires_external, "No support for external requirements"
        assert not info.supported_platforms, "No support for supported platforms field"

        spec = api.Spec()
        spec.pkg.name = _to_spk_name(info.name)
        spec.pkg.version = api.parse_version(_to_spk_version(info.version))
        spec.sources = []
        spec.build.options = [
            api.VarOpt("os"),
            api.VarOpt("arch"),
            api.VarOpt("distro"),
            api.PkgOpt("python", self._python_version),
        ]
        spec.build.script = "\n".join(
            [
                "export PYTHONNOUSERSITE=1",
                "export PYTHONDONTWRITEBYTECODE=1",
                f"/spfs/bin/python -BEs -m pip install {info.name}=={info.version} --no-deps",
            ]
        )

        builds = []
        if info.requires_python:
            _LOGGER.debug(
                "ignoring defined python range, other version of python will need to have this package reconverted"
            )
        # python packages can support a wide range of versions, and present dynamic
        # requirements based on the version used - spk does not do this so instead
        # we restrict the package to the python version that it's being imported for
        spec.install.requirements.append(
            api.PkgRequest(api.parse_ident_range(f"python/{self._python_version}"))
        )
        for requirement_str in info.requires_dist:

            if ";" not in requirement_str:
                requirement_str += ";"
            version_str, markers_str = requirement_str.split(";", 1)

            if markers_str.strip():
                marker = packaging.markers.Marker(markers_str)
                if not marker.evaluate(
                    {"extra": "", "python_version": self._python_version}
                ):
                    _LOGGER.debug(
                        "Skip requirement due to markers", requirement=requirement_str
                    )
                    continue

            _LOGGER.debug("converting dependency requirement", requirement=version_str)
            match = re.match(r"([^ ]+)( \((.*)\))?", version_str.strip())
            assert match, f"Failed to parse requirement string: {version_str}"
            spk_name = _to_spk_name(match.group(1))
            if spk_name in BAKED_PYTHON_PACKAGES:
                _LOGGER.warning(
                    f"skipping requirement for {spk_name}, this package cannot be updated with the "
                    "pip conversion since it's baked into the spk python package"
                )
                continue
            spk_version_range = _to_spk_version_range(match.group(3) or "*")
            request = api.PkgRequest(
                api.parse_ident_range(f"{spk_name}/{spk_version_range}")
            )
            spec.install.requirements.append(request)

            if self._follow_deps:
                _LOGGER.debug("following dependencies...")
                builds.extend(self.import_package(match.group(1), match.group(3) or ""))

        repo = storage.local_repository()
        options = api.host_options()
        if self._python_abi is not None:
            options["python.abi"] = self._python_abi
        _LOGGER.info("building generated package spec...", pkg=spec.pkg)
        builder = build.BinaryPackageBuilder().from_spec(spec)
        try:
            created = (
                builder.with_options(options)
                .with_repository(repo)
                .with_repository(storage.remote_repository())
                .with_source(".")
                .build()
            )
        except solve.SolverError:
            print(io.format_solve_graph(builder.get_solve_graph(), verbosity=100))
            raise
        builds.insert(0, created)

        return builds


def _to_spk_name(name: str) -> str:

    return name.lower().replace("_", "-").replace(".", "-")


def _to_spk_version(version: str) -> str:

    python_version = packaging.version.parse(version)
    spk_version = api.parse_version(python_version.base_version)
    if python_version.pre is not None:
        name, num = python_version.pre
        spk_version.pre[name] = num
    if python_version.dev is not None:
        spk_version.pre["dev"] = int(python_version.dev)  # type: ignore
    if python_version.post is not None:
        spk_version.post["post"] = int(python_version.post)  # type: ignore
    if python_version.local:
        # irrelevant information for compatibility of versions and
        # no equal concept in spk versions specs
        pass

    return spk_version.__str__().replace(spk_version.base, python_version.base_version)


def _to_spk_version_range(version_range: str) -> api.VersionRange:

    version_range = version_range.replace(" ", "").strip(",")
    versions = version_range.split(",")
    for i, version in enumerate(versions):

        stripped = version.lstrip("><=!~")
        prefix = version[: -len(stripped)]
        if "*" not in version:
            # handle pre and post release tags added to version numbers if possible
            converted = _to_spk_version(stripped).__str__()
        else:
            converted = stripped
        version = _to_spk_range_prefix(prefix) + converted

        # we cannot combine '~=' and *, but a trailing * is the
        # most common and is semantically equal to the same version
        # without a wildcard
        # !=3.7.* ==> !=3.7
        if version[0] in {"!", "="} and version.endswith(".*"):
            version = f"{version[:-2]}"
        # spk uses a single equals sign for exact version, where pip
        # would use a double
        # ==1.4.0 --> =1.4.0
        if version.startswith("=="):
            version = version[1:]
        versions[i] = version

    return api.parse_version_range(",".join(versions))


def _to_spk_range_prefix(prefix: str) -> str:

    pip_to_spk = {
        ">": ">",
        "<": "<",
        ">=": ">=",
        "<=": "<=",
        "==": "=",
        "~=": "~",
        "!=": "!=",
        "": "",
    }
    try:
        return pip_to_spk[prefix]
    except KeyError:
        raise ValueError(f"Unhandled pip version range prefix: {prefix}")
