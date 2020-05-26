from typing import Dict, List
import abc

import spfs

from .. import api
from ._repository import Repository, VersionExistsError, PackageNotFoundError


class MemRepository(Repository):
    def __init__(self) -> None:
        self._specs: Dict[str, Dict[str, api.Spec]] = {}
        self._sources: Dict[str, Dict[str, spfs.encoding.Digest]] = {}
        self._packages: Dict[str, Dict[str, Dict[str, spfs.encoding.Digest]]] = {}

    def list_packages(self) -> List[str]:
        return list(self._specs.keys())

    def list_package_versions(self, name: str) -> List[str]:

        try:
            return list(self._specs[name].keys())
        except KeyError:
            return []

    def read_spec(self, pkg: api.Ident) -> api.Spec:

        try:
            return self._specs[pkg.name][str(pkg.version)]
        except KeyError:
            raise PackageNotFoundError(pkg)

    def get_package(self, pkg: api.Ident) -> spfs.encoding.Digest:

        if pkg.build is None:
            raise PackageNotFoundError(pkg)
        try:
            return self._packages[pkg.name][str(pkg.version)][pkg.build.digest]
        except KeyError:
            raise PackageNotFoundError(pkg)

    def get_source_package(self, pkg: api.Ident,) -> spfs.encoding.Digest:

        try:
            return self._sources[pkg.name][str(pkg.version)]
        except KeyError:
            raise PackageNotFoundError(pkg)

    def force_publish_spec(self, spec: api.Spec) -> None:

        try:
            del self._specs[spec.pkg.name][str(spec.pkg.version)]
        except KeyError:
            pass
        self.publish_spec(spec)

    def publish_spec(self, spec: api.Spec) -> None:

        self._specs.setdefault(spec.pkg.name, {})
        versions = self._specs[spec.pkg.name]
        version = str(spec.pkg.version)
        if version in versions:
            raise VersionExistsError(version)
        versions[version] = spec

    def publish_package(self, pkg: api.Ident, digest: spfs.encoding.Digest) -> None:

        if pkg.build is None:
            raise ValueError(
                "Package must include a build in order to be published: " + str(pkg)
            )

        self._packages.setdefault(pkg.name, {})
        version = str(pkg.version)
        self._packages[pkg.name].setdefault(version, {})
        build = pkg.build.digest
        self._packages[pkg.name][version][build] = digest
