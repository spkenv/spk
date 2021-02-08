from abc import ABCMeta, abstractmethod
from typing import List, Dict, Optional, Iterator, Tuple, Iterator, Tuple, TypeVar, Set
from typing_extensions import Protocol, runtime_checkable

import structlog

from .. import api, storage
from ._errors import PackageNotFoundError
from ._solution import PackageSource

_LOGGER = structlog.get_logger("spk.solve")


Self = TypeVar("Self")


class PackageIterator(Iterator[Tuple[api.Spec, PackageSource]], metaclass=ABCMeta):
    @abstractmethod
    def clone(self: Self) -> Self:
        ...


class RepositoryPackageIterator(PackageIterator):
    """A stateful cursor yielding package builds from a set of repositories."""

    def __init__(self, package_name: str, repos: List[storage.Repository],) -> None:
        self._package_name = package_name
        self._repos = repos
        self._versions: Optional[Iterator[api.Version]] = None
        self._builds: Iterator[api.Ident] = iter([])
        self._version_map: Dict[api.Version, storage.Repository] = {}

    def _start(self) -> None:

        self._version_map = {}
        for repo in reversed(self._repos):
            repo_versions = repo.list_package_versions(self._package_name)
            for version_str in repo_versions:
                version = api.parse_version(version_str)
                self._version_map[version] = repo

        if len(self._version_map) == 0:
            raise PackageNotFoundError(self._package_name)

        versions = list(self._version_map.keys())
        versions.sort()
        versions.reverse()
        self._versions = iter(versions)
        self._builds = iter([])

    def clone(self) -> "RepositoryPackageIterator":
        """Create a copy of this iterator, with the cursor at the same point."""

        other = RepositoryPackageIterator(self._package_name, self._repos)
        if self._versions is None:
            try:
                self._start()
            except PackageNotFoundError:
                return other

        remaining_versions = list(self._versions or [])
        remaining_builds = list(self._builds or [])
        other._versions = iter(remaining_versions)
        self._versions = iter(remaining_versions)
        other._builds = iter(remaining_builds)
        self._builds = iter(remaining_builds)
        other._version_map = self._version_map.copy()
        return other

    def __next__(self) -> Tuple[api.Spec, PackageSource]:

        if self._versions is None:
            self._start()

        for build in self._builds:

            repo = self._version_map[build.version]
            try:
                spec = repo.read_spec(build)
            except storage.PackageNotFoundError:
                _LOGGER.warning(
                    f"Repository listed build with no spec: {build} from {repo}"
                )
                continue
            if spec.pkg.build is None:
                _LOGGER.debug(
                    "Published spec is corrupt (has no associated build)", pkg=build
                )
                spec.pkg.build = build.build

            return (spec, repo)

        version = next(self._versions or iter([]))
        repo = self._version_map[version]
        builds = list(repo.list_package_builds(api.Ident(self._package_name, version)))
        # source packages must come last to ensure that building
        # from source is the last option under normal circumstances
        builds.sort(key=lambda pkg: pkg.is_source())
        self._builds = iter(builds)

        return next(self)
