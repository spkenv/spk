from typing import Dict, Iterable, Union, Tuple
import abc

import spfs

from .. import api
from ._repository import Repository, VersionExistsError, PackageNotFoundError


class MemRepository(Repository):
    def __init__(self) -> None:
        self._specs: Dict[str, Dict[str, api.Spec]] = {}
        self._packages: Dict[
            str, Dict[str, Dict[str, Tuple[api.Spec, spfs.encoding.Digest]]]
        ] = {}

    def list_packages(self) -> Iterable[str]:
        return list(self._specs.keys())

    def list_package_versions(self, name: str) -> Iterable[str]:

        try:
            return list(self._specs[name].keys())
        except KeyError:
            return []

    def list_package_builds(self, pkg: Union[str, api.Ident]) -> Iterable[api.Ident]:

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        pkg = pkg.with_build(None)
        try:
            for build in self._packages[pkg.name][str(pkg.version)].keys():
                yield pkg.with_build(build)
        except KeyError:
            return []

    def read_spec(self, pkg: api.Ident) -> api.Spec:

        try:
            if not pkg.build:
                return self._specs[pkg.name][str(pkg.version)].clone()
            else:
                return self._packages[pkg.name][str(pkg.version)][pkg.build.digest][
                    0
                ].clone()
        except KeyError:
            raise PackageNotFoundError(pkg)

    def get_package(self, pkg: api.Ident) -> spfs.encoding.Digest:

        if pkg.build is None:
            raise PackageNotFoundError(pkg)
        try:
            return self._packages[pkg.name][str(pkg.version)][pkg.build.digest][1]
        except KeyError:
            raise PackageNotFoundError(pkg)

    def force_publish_spec(self, spec: api.Spec) -> None:

        try:
            del self._specs[spec.pkg.name][str(spec.pkg.version)]
        except KeyError:
            pass
        self.publish_spec(spec)

    def publish_spec(self, spec: api.Spec) -> None:

        assert spec.pkg.build is None, "Spec must be published with no build"
        assert (
            spec.pkg.build is None or not spec.pkg.build.is_emdedded()
        ), "Cannot publish embedded package"
        self._specs.setdefault(spec.pkg.name, {})
        versions = self._specs[spec.pkg.name]
        version = str(spec.pkg.version)
        if version in versions:
            raise VersionExistsError(version)
        versions[version] = spec.clone()

    def remove_spec(self, pkg: api.Ident) -> None:

        try:
            del self._specs[pkg.name][str(pkg.version)]
        except KeyError:
            raise PackageNotFoundError(pkg)

    def publish_package(self, spec: api.Spec, digest: spfs.encoding.Digest) -> None:

        if spec.pkg.build is None:
            raise ValueError(
                "Package must include a build in order to be published: "
                + str(spec.pkg)
            )

        self._packages.setdefault(spec.pkg.name, {})
        version = str(spec.pkg.version)
        self._packages[spec.pkg.name].setdefault(version, {})
        build = spec.pkg.build.digest
        self._packages[spec.pkg.name][version][build] = (spec.clone(), digest)

    def remove_package(self, pkg: api.Ident) -> None:

        if pkg.build is None:
            raise ValueError(
                "Package must include a build in order to be removed: " + str(pkg)
            )
        try:
            del self._packages[pkg.name][str(pkg.version)][pkg.build.digest]
        except KeyError:
            raise PackageNotFoundError(pkg)
