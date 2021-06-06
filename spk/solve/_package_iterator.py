# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import List, Dict, Optional, Iterator, Tuple, Iterator, Tuple, TypeVar
from abc import ABCMeta, abstractmethod

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

    @abstractmethod
    def is_empty(self) -> bool:
        pass

    def version_spec(self) -> Optional[api.Spec]:
        return None


class PackageIterator(Iterator[Tuple[api.Ident, BuildIterator]], metaclass=ABCMeta):
    """Iterates the versions of a package, yielding stateful iterators for each build.

    Until all builds are iterated, the package iterator should not yield a new version
    """

    @abstractmethod
    def clone(self: Self) -> Self:
        ...

    @abstractmethod
    def set_builds(self, version: api.Version, builds: BuildIterator) -> None:
        """Replaces the internal build iterator for version with the given one."""
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
        self._builds_map: Dict[api.Version, BuildIterator] = {}
        self._active_version: Optional[api.Version] = None

    def set_builds(self, version: api.Version, builds: BuildIterator) -> None:
        self._builds_map[version] = builds

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

        if self._active_version is None:
            self._active_version = next(self._versions or iter([]))
        version = self._active_version
        repo = self._version_map[version]
        pkg = api.Ident(self._package_name, version)
        if version not in self._builds_map:
            self._builds_map[version] = RepositoryBuildIterator(pkg, repo)
        builds = self._builds_map[version]
        if builds.is_empty():
            self._active_version = None
            return next(self)
        return (pkg, builds)


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
        self._builds.sort(key=lambda pkg: not pkg.is_source())

    def version(self) -> api.Version:
        return self._pkg.version

    def is_empty(self) -> bool:
        return bool(len(self._builds) == 0)

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
            spec.pkg = spec.pkg.with_build(build.build)

        return (spec, self._repo)


class EmptyBuildIterator(BuildIterator):
    def version(self) -> api.Version:
        return api.Version()

    def is_empty(self) -> bool:
        return True

    def __next__(self) -> Tuple[api.Spec, PackageSource]:
        raise StopIteration


class SortedBuildIterator(BuildIterator):
    def __init__(self, options: api.OptionMap, source: BuildIterator) -> None:
        self._options = options
        self._source = source
        self._builds = list(source)
        self.sort()

    def version(self) -> api.Version:
        return self._source.version()

    def is_empty(self) -> bool:
        return bool(len(self._builds) == 0)

    def version_spec(self) -> Optional[api.Spec]:
        return self._source.version_spec()

    def sort(self) -> None:

        version_spec = self.version_spec()
        variant_count = len(version_spec.build.variants) if version_spec else 0
        default_options = (
            version_spec.resolve_all_options(api.OptionMap())
            if version_spec
            else api.OptionMap()
        )

        def key(entry: Tuple[api.Spec, PackageSource]) -> Tuple[int, str]:

            spec, _ = entry
            build = str(spec.pkg.build)
            total_options_count = len(spec.build.options)
            # source packages must come last to ensure that building
            # from source is the last option under normal circumstances
            if spec.pkg.build is None or spec.pkg.build == api.SRC:
                return (variant_count + total_options_count + 1, build)

            if version_spec is not None:
                # if this spec is compatible with the default options, it's the
                # most valuable
                if spec.build.validate_options(spec.pkg.name, default_options):
                    return (-1, build)
                # then we sort based on the first defined variant that seems valid
                for (i, variant) in enumerate(version_spec.build.variants):
                    if spec.build.validate_options(spec.pkg.name, variant):
                        return (i, build)

            # and then it's the distance from the default option set,
            # where distance is just the number of differing options
            current_options = dict(
                (o, v)
                for o, v in spec.resolve_all_options(api.OptionMap()).items()
                if o in self._options
            )
            similar_options_count = len(
                set(default_options.items()) & set(current_options.items())
            )
            distance_from_default = max(0, total_options_count - similar_options_count)
            return (variant_count + distance_from_default, build)

        self._builds.sort(key=key)

    def __next__(self) -> Tuple[api.Spec, PackageSource]:

        try:
            return self._builds.pop(0)
        except IndexError:
            raise StopIteration
