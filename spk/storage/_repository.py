# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, Iterable, Union
import abc

import spkrs

from .. import api


class VersionExistsError(FileExistsError):
    def __init__(self, pkg: Any) -> None:
        super(VersionExistsError, self).__init__(
            f"Package version already exists: {pkg}"
        )


class PackageNotFoundError(FileNotFoundError):
    def __init__(self, pkg: Any) -> None:
        super(PackageNotFoundError, self).__init__(f"Package not found: {pkg}")


class Repository(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def list_packages(self) -> Iterable[str]:
        """Return the set of known packages in this repo."""
        pass

    @abc.abstractmethod
    def list_package_versions(self, name: str) -> Iterable[str]:
        """Return the set of versions available for the named package."""
        pass

    @abc.abstractmethod
    def list_package_builds(self, pkg: Union[str, api.Ident]) -> Iterable[api.Ident]:
        """Return the set of builds for the given package name and version."""
        pass

    @abc.abstractmethod
    def read_spec(self, pkg: api.Ident) -> api.Spec:
        """Read a package spec file for the given package, version and optional build.

        Raises
            PackageNotFoundError: If the package, version, or build does not exist
        """
        pass

    @abc.abstractmethod
    def get_package(self, pkg: api.Ident) -> spkrs.Digest:
        """Identify the payload for the identified binary package and build options.

        The given build options should be resolved using the package spec
        before calling this function, unless the exact complete set of options
        can be known deterministically.
        """

        pass

    @abc.abstractmethod
    def publish_spec(self, spec: api.Spec) -> None:
        """Publish a package spec to this repository.

        The published spec represents all builds of a single version.
        The source package, or at least one binary package should be
        published as well in order to make the spec usable in environments.

        Raises:
            VersionExistsError: if the spec a this version is already present
        """
        pass

    @abc.abstractmethod
    def remove_spec(self, pkg: api.Ident) -> None:
        """Remove a package version from this repository.

        This will not untag builds for this package, but make it unresolvable
        and unsearchable. It's recommended that you remove all existing builds
        before removing the spec in order to keep the repository clean.
        """
        pass

    @abc.abstractmethod
    def force_publish_spec(self, spec: api.Spec) -> None:
        """Publish a package spec to this repository.

        Same as 'publish_spec' except that it clobbers any existing
        spec at this version
        """
        pass

    @abc.abstractmethod
    def publish_package(self, spec: api.Spec, digest: spkrs.Digest) -> None:
        """Publish a binary package to this repository.

        The published digest is expected to identify an spfs layer which contains
        the propery constructed binary package files and metadata.
        """
        pass

    @abc.abstractmethod
    def remove_package(self, pkg: api.Ident) -> None:
        """Remove a package from this repository.

        The given package identifier must identify a full package build
        """
        pass
