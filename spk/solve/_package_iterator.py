from typing import List, Dict, Optional, Iterator, Tuple

import structlog

from .. import api, storage
from ._errors import PackageNotFoundError

_LOGGER = structlog.get_logger("spk.solve")


class PackageIterator(Iterator[Tuple[api.Spec, storage.Repository]]):
    """PackageIterator is a stateful cursor yielding package versions.

    The iterator yields packages from a repository which are compatible with some
    request. These are used to retain a cursor in the repo in the case of
    needing to continue with next-best option upon error or issue in the solve.
    """

    def __init__(
        self,
        repos: List[storage.Repository],
        request: api.Request,
        options: api.OptionMap,
    ) -> None:
        self._repos = repos
        self._request = request
        self._options = options
        self._versions: Optional[Iterator[str]] = None
        self._builds: Optional[Iterator[api.Ident]] = None
        self._version_map: Dict[str, storage.Repository] = {}
        self._version_spec: Optional[api.Spec] = None
        self.history: Dict[api.Ident, api.Compatibility] = {}

    def _start(self) -> None:

        self._version_map = {}
        for repo in reversed(self._repos):
            versions = repo.list_package_versions(self._request.pkg.name)
            for version in versions:
                self._version_map[version] = repo

        if not len(self._version_map):
            raise PackageNotFoundError(self._request.pkg.name)

        versions = list(self._version_map.keys())
        versions.sort()
        versions.reverse()
        self._versions = iter(versions)
        self._builds = iter([])

    def clone(self) -> "PackageIterator":
        """Create a copy of this iterator, with the cursor at the same point."""

        if self._versions is None:
            self._start()

        other = PackageIterator(self._repos, self._request, self._options)
        remaining_versions = list(self._versions or [])
        remaining_builds = list(self._builds or [])
        other._versions = iter(remaining_versions)
        self._versions = iter(remaining_versions)
        self._builds = iter(remaining_builds)
        other._builds = iter(remaining_builds)
        other.history = self.history.copy()
        other._version_map = self._version_map
        other._version_spec = self._version_spec
        return other

    def __next__(self) -> Tuple[api.Spec, storage.Repository]:

        if self._versions is None:
            self._start()

        requested_build = self._request.pkg.build
        for candidate in self._builds or []:

            if requested_build is not None:
                if requested_build != candidate.build:
                    continue
            if candidate.build is None:
                _LOGGER.error(
                    "published package has no associated build", pkg=candidate
                )
                continue

            version_str = str(candidate.version)
            repo = self._version_map[version_str]

            if requested_build is None and candidate.build.is_source():
                if self._version_spec is None:
                    self.history[candidate] = api.Compatibility(
                        "No version-level spec, cannot rebuild from source"
                    )
                    continue
                spec = self._version_spec
            else:
                spec = repo.read_spec(candidate)

            compat = self._request.is_satisfied_by(spec)
            if not compat:
                self.history[candidate] = compat
                continue

            compat = spec.build.validate_options(self._options)
            if not compat:
                self.history[candidate] = compat
                continue

            return (spec, repo)

        self._start_next_version()
        return self.__next__()

    def _start_next_version(self) -> None:

        for version_str in self._versions or []:
            version = api.parse_version(version_str)

            compat = self._request.is_version_applicable(version)
            if not compat:
                self.history[api.Ident(self._request.pkg.name, version)] = compat
                continue

            pkg = api.Ident(self._request.pkg.name, version)
            repo = self._version_map[version_str]
            try:
                self._version_spec = repo.read_spec(pkg)
            except storage.PackageNotFoundError:
                _LOGGER.debug("package has no verison spec", pkg=pkg, repo=repo)
                self._version_spec = None
            else:
                compat = self._version_spec.build.validate_options(self._options)
                if not compat:
                    self.history[api.Ident(self._request.pkg.name, version)] = compat
                    continue

                compat = self._request.pkg.version.is_satisfied_by(self._version_spec)
                if not compat:
                    self.history[api.Ident(self._request.pkg.name, version)] = compat
                    continue

            builds = list(repo.list_package_builds(pkg))
            # source packages must come last to ensure that building
            # from source is the last option under normal circumstances
            builds.sort(key=lambda pkg: pkg.build and pkg.build.is_source())
            self._builds = iter(builds)
            return

        raise StopIteration
