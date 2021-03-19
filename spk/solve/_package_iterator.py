from abc import ABCMeta, abstractmethod
from typing import List, Dict, Optional, Iterator, Tuple, Iterator, Tuple, TypeVar, Set
from typing_extensions import Protocol, runtime_checkable

import structlog

from .. import api, storage
from ._errors import PackageNotFoundError
from ._solution import PackageSource

_LOGGER = structlog.get_logger("spk.solve")


Self = TypeVar("Self")


class BuildIterator(Iterator[Tuple[api.Spec, PackageSource]], metaclass=ABCMeta):
    @abstractmethod
    def version(self) -> api.Version:
        pass

    def version_spec(self) -> Optional[api.Spec]:
        return None


class PackageIterator(Iterator[Tuple[api.Ident, BuildIterator]], metaclass=ABCMeta):
    @abstractmethod
    def clone(self: Self) -> Self:
        ...


class RepositoryPackageIterator(PackageIterator):
    """A stateful cursor yielding package builds from a set of repositories."""

    def __init__(
        self,
        package_name: str,
        repos: List[storage.Repository],
    ) -> None:
        self._package_name = package_name
        self._repos = repos
        self._versions: Optional[Iterator[api.Version]] = None
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

    def clone(self) -> "RepositoryPackageIterator":
        """Create a copy of this iterator, with the cursor at the same point."""

        other = RepositoryPackageIterator(self._package_name, self._repos)
        if self._versions is None:
            try:
                self._start()
            except PackageNotFoundError:
                return other

        remaining_versions = list(self._versions or [])
        other._versions = iter(remaining_versions)
        self._versions = iter(remaining_versions)
        other._version_map = self._version_map.copy()
        return other

    def __next__(self) -> Tuple[api.Ident, BuildIterator]:

        if self._versions is None:
            self._start()

        version = next(self._versions or iter([]))
        repo = self._version_map[version]
        pkg = api.Ident(self._package_name, version)
        return (pkg, RepositoryBuildIterator(pkg, repo))


class RepositoryBuildIterator(BuildIterator):
    def __init__(self, pkg: api.Ident, repo: storage.Repository) -> None:
        self._pkg = pkg
        self._repo = repo
        self._builds = list(repo.list_package_builds(pkg))
        self._spec: Optional[api.Spec] = None
        try:
            self._spec = repo.read_spec(pkg)
        except storage.PackageNotFoundError:
            pass
        # source packages must come last to ensure that building
        # from source is the last option under normal circumstances
        self._builds.sort(key=lambda pkg: pkg.is_source())

    def version(self) -> api.Version:
        return self._pkg.version

    def version_spec(self) -> Optional[api.Spec]:
        return self._spec

    def __next__(self) -> Tuple[api.Spec, PackageSource]:

        try:
            build = self._builds.pop(0)
        except IndexError:
            raise StopIteration
        try:
            spec = self._repo.read_spec(build)
        except storage.PackageNotFoundError:
            _LOGGER.warning(
                f"Repository listed build with no spec: {build} from {self._repo}"
            )
            return next(self)
        if spec.pkg.build is None:
            _LOGGER.debug(
                "Published spec is corrupt (has no associated build)", pkg=build
            )
            spec.pkg.build = build.build

        return (spec, self._repo)
