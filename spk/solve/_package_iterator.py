from abc import ABCMeta, abstractmethod
from typing import List, Dict, Optional, Iterator, Tuple, Iterator, Tuple, TypeVar, Set
from typing_extensions import Protocol, runtime_checkable

import structlog

from .. import api, storage
from ._errors import PackageNotFoundError
from ._solution import PackageSource

_LOGGER = structlog.get_logger("spk.solve")


PackageIterator = Iterator[Tuple[api.Spec, PackageSource]]
Self = TypeVar("Self")


@runtime_checkable
class Cloneable(Protocol):
    def clone(self: Self) -> Self:
        pass


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

        if not len(self._version_map):
            raise PackageNotFoundError(self._package_name)

        versions = list(self._version_map.keys())
        versions.sort()
        versions.reverse()
        self._versions = iter(versions)
        self._builds = iter([])

    def clone(self) -> "RepositoryPackageIterator":
        """Create a copy of this iterator, with the cursor at the same point."""

        if self._versions is None:
            self._start()

        other = RepositoryPackageIterator(self._package_name, self._repos)
        remaining_versions = list(self._versions or [])
        remaining_builds = list(self._builds or [])
        other._versions = iter(remaining_versions)
        self._versions = iter(remaining_versions)
        self._builds = iter(remaining_builds)
        other._version_map = self._version_map.copy()
        other._builds = iter(remaining_builds)
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
        builds.sort(key=lambda pkg: bool(pkg.build and pkg.build.is_source()))
        self._builds = iter(builds)

        return next(self)


class FilteredPackageIterator(PackageIterator):
    """Filters a stream of packages through a request.

    The iterator yields only packages which are compatible with a given
    request. These are used to retain a cursor in the repo in the case of
    needing to continue with next-best option upon error or issue in the solve.
    """

    def __init__(
        self, source: PackageIterator, request: api.Request, options: api.OptionMap,
    ) -> None:
        self._source = source
        self.request = request
        self.options = options
        self.history: Dict[api.Ident, api.Compatibility] = {}
        self._visited_versions: Dict[str, api.Version] = {}

    def __next__(self) -> Tuple[api.Spec, PackageSource]:

        requested_build = self.request.pkg.build
        for candidate, repo in self._source:

            base_version = candidate.pkg.version.base
            if base_version in self._visited_versions:
                # if we have already visited 1.0.0 with some release
                # we don't want to visit any other releases, but we do
                # want to visit other builds of the same release
                if candidate.pkg.version != self._visited_versions[base_version]:
                    continue

            # check version number without build
            compat = self.request.is_version_applicable(candidate.pkg.version)
            if not compat:
                self.history[candidate.pkg.with_build(None)] = compat
                continue

            # FIXME: loading this for each build is not efficient
            version_spec: Optional[api.Spec] = None
            try:
                assert isinstance(repo, storage.Repository)
                version_spec = repo.read_spec(candidate.pkg.with_build(None))
            except (AssertionError, storage.PackageNotFoundError):
                _LOGGER.debug(
                    "package has no version spec", pkg=candidate.pkg, repo=repo
                )

            # check option compatibility of entire version, if applicable
            if version_spec is not None:
                opts = version_spec.build.resolve_all_options(self.options)
                compat = version_spec.build.validate_options(opts)
                if not compat:
                    self.history[candidate.pkg.with_build(None)] = compat
                    continue

                compat = self.request.pkg.version.is_satisfied_by(version_spec)
                if not compat:
                    self.history[candidate.pkg.with_build(None)] = compat
                    continue

            if requested_build is not None:
                if requested_build != candidate.pkg.build:
                    self.history[candidate.pkg.with_build(None)] = api.Compatibility(
                        f"Exact build was requested: {candidate.pkg.build} != {requested_build}"
                    )
                    continue
            if candidate.pkg.build is None:
                self.history[candidate.pkg] = api.Compatibility(
                    "Package is corrupt (has no associated build)"
                )
                continue

            if requested_build is None and candidate.pkg.build.is_source():
                if version_spec is not None:
                    spec = version_spec
                else:
                    self.history[candidate.pkg] = api.Compatibility(
                        "No version-level spec, cannot rebuild from source"
                    )
                    continue
            else:
                spec = candidate

            # FIXME: this does not seem like the right place for this logic
            if version_spec is not None:
                if version_spec.deprecated:
                    spec.deprecated = True

            compat = self.request.is_satisfied_by(spec)
            if not compat:
                self.history[candidate.pkg] = compat
                continue

            # resolve and update missing options from spec in case there
            # are options with a default value that need to be set before
            # validating
            opts = self.options.copy()
            for name, value in spec.build.resolve_all_options(opts).items():
                opts.setdefault(name, value)
            compat = spec.build.validate_options(opts)
            if not compat:
                self.history[candidate.pkg] = compat
                continue

            self._visited_versions[candidate.pkg.version.base] = candidate.pkg.version
            return (spec, repo)

        raise StopIteration
