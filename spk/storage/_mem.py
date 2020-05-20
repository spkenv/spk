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

    def get_package(
        self, pkg: api.Ident, options: api.OptionMap
    ) -> spfs.encoding.Digest:

        try:
            return self._packages[pkg.name][str(pkg.version)][options.digest()]
        except KeyError:
            raise PackageNotFoundError(pkg)

    def get_source_package(self, pkg: api.Ident,) -> spfs.encoding.Digest:

        try:
            return self._sources[pkg.name][str(pkg.version)]
        except KeyError:
            raise PackageNotFoundError(pkg)

    def publish_spec(self, spec: api.Spec) -> None:

        self._specs.setdefault(spec.pkg.name, {})
        versions = self._specs[spec.pkg.name]
        version = str(spec.pkg.version)
        if version in versions:
            raise VersionExistsError(version)
        versions[version] = spec

    def publish_package(
        self, pkg: api.Ident, options: api.OptionMap, digest: spfs.encoding.Digest
    ) -> None:

        self._packages.setdefault(pkg.name, {})
        version = str(pkg.version)
        self._packages[pkg.name].setdefault(version, {})
        build = options.digest()
        self._packages[pkg.name][version][build] = digest

    def publish_source_package(
        self, pkg: api.Ident, digest: spfs.encoding.Digest
    ) -> None:

        self._sources.setdefault(pkg.name, {})
        version = str(pkg.version)
        self._sources[pkg.name][version] = digest
