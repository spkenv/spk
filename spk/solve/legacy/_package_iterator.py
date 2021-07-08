# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from abc import ABCMeta, abstractmethod
from typing import List, Dict, Optional, Iterator, Tuple, Iterator, Tuple, TypeVar

import structlog

from ... import api, storage
from .. import PackageSource, PackageNotFoundError

_LOGGER = structlog.get_logger("spk.solve")


Self = TypeVar("Self")


class PackageIterator(Iterator[Tuple[api.Spec, PackageSource]], metaclass=ABCMeta):
    @abstractmethod
    def get_history(self) -> Dict[api.Ident, api.Compatibility]:
        ...

    @abstractmethod
    def add_history(self, ident: api.Ident, compat: api.Compatibility) -> None:
        ...

    @abstractmethod
    def clone(self: Self) -> Self:
        ...


class ListPackageIterator(PackageIterator):
    def __init__(
        self,
        packages: List[Tuple[api.Spec, PackageSource]],
        history_source: PackageIterator = None,
    ) -> None:
        self._packages = iter(packages)
        self._history_source = history_source
        self._history: Dict[api.Ident, api.Compatibility] = {}

    def get_history(self) -> Dict[api.Ident, api.Compatibility]:
        h = self._history.copy()
        if self._history_source is not None:
            h.update(self._history_source.get_history())
        return h

    def add_history(self, ident: api.Ident, compat: api.Compatibility) -> None:
        self._history[ident] = compat

    def __next__(self) -> Tuple[api.Spec, PackageSource]:
        return next(self._packages)

    def clone(self) -> "ListPackageIterator":
        dupe = list(self._packages)
        self._packages = iter(dupe)
        it = ListPackageIterator(dupe, self._history_source)
        it._history = self._history.copy()
        return it


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
        self._builds: Iterator[api.Ident] = iter([])
        self._version_map: Dict[api.Version, storage.Repository] = {}
        self._history: Dict[api.Ident, api.Compatibility] = {}

    def get_history(self) -> Dict[api.Ident, api.Compatibility]:
        return self._history.copy()

    def add_history(self, ident: api.Ident, compat: api.Compatibility) -> None:
        self._history[ident] = compat

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
        other._history = self._history.copy()
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
                spec.pkg = spec.pkg.with_build(build.build)

            return (spec, repo)

        version = next(self._versions or iter([]))
        repo = self._version_map[version]
        builds = list(repo.list_package_builds(api.Ident(self._package_name, version)))
        # source packages must come last to ensure that building
        # from source is the last option under normal circumstances
        builds.sort(key=lambda pkg: bool(pkg.build and pkg.build == "SRC"))
        self._builds = iter(builds)

        return next(self)


class FilteredPackageIterator(PackageIterator):
    """Filters a stream of packages through a request.

    The iterator yields only packages which are compatible with a given
    request. These are used to retain a cursor in the repo in the case of
    needing to continue with next-best option upon error or issue in the solve.
    """

    def __init__(
        self,
        source: PackageIterator,
        request: api.PkgRequest,
        options: api.OptionMap,
    ) -> None:
        self._source = source
        self.request = request
        self.options = options
        self._history: Dict[api.Ident, api.Compatibility] = {}
        self._visited_versions: Dict[str, api.Version] = {}

    def get_history(self) -> Dict[api.Ident, api.Compatibility]:
        h = self._source.get_history()
        h.update(self._history)
        return h

    def add_history(self, ident: api.Ident, compat: api.Compatibility) -> None:
        self._history[ident] = compat

    def clone(self) -> "FilteredPackageIterator":

        it = FilteredPackageIterator(
            self._source.clone(), self.request.copy(), self.options.copy()
        )
        it._history = self._history.copy()
        return it

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
                self.add_history(candidate.pkg.with_build(None), compat)
                continue

            # FIXME: loading this for each build is not efficient
            version_spec: Optional[api.Spec] = None
            try:
                assert isinstance(repo, storage.Repository)
                version_spec = repo.read_spec(candidate.pkg.with_build(None))
            except (AssertionError, storage.PackageNotFoundError):
                # package has no version spec, which will stop us checking
                # some options efficiently, but is not really a problem
                pass

            # check option compatibility of entire version, if applicable
            if version_spec is not None:
                compat = api.version_range_is_satisfied_by(
                    self.request.pkg.version, version_spec
                )
                if not compat:
                    self.add_history(candidate.pkg.with_build(None), compat)
                    continue

                opts = version_spec.resolve_all_options(self.options)
                compat = version_spec.build.validate_options(
                    version_spec.pkg.name, opts
                )
                if not compat:
                    self.add_history(candidate.pkg.with_build(None), compat)
                    continue

            if requested_build is not None:
                if requested_build != candidate.pkg.build:
                    self.add_history(
                        candidate.pkg.with_build(None),
                        api.Compatibility(
                            f"Exact build was requested: {candidate.pkg.build} != {requested_build}"
                        ),
                    )
                    continue
            if candidate.pkg.build is None:
                self.add_history(
                    candidate.pkg,
                    api.Compatibility("Package is corrupt (has no associated build)"),
                )
                continue

            if requested_build is None and candidate.pkg.build == api.SRC:
                if version_spec is not None:
                    spec = version_spec
                else:
                    self.add_history(
                        candidate.pkg,
                        api.Compatibility(
                            "No version-level spec, cannot rebuild from source"
                        ),
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
                self.add_history(candidate.pkg, compat)
                continue

            # resolve and update missing options from spec in case there
            # are options with a default value that need to be set before
            # validating
            opts = self.options.copy()
            for name, value in spec.resolve_all_options(opts).items():
                if name not in opts:
                    opts[name] = value
            compat = spec.build.validate_options(spec.pkg.name, self.options)
            if not compat:
                self.add_history(candidate.pkg, compat)
                continue

            self._visited_versions[candidate.pkg.version.base] = candidate.pkg.version
            return (spec, repo)

        raise StopIteration
