from typing import List
import abc

import spfs

from .. import api


class VersionExistsError(FileExistsError):
    pass


class PackageNotFoundError(FileNotFoundError):
    pass


class Repository(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def list_packages(self) -> List[str]:
        pass

    @abc.abstractmethod
    def list_package_versions(self, name: str) -> List[str]:
        pass

    @abc.abstractmethod
    def read_spec(self, pkg: api.Ident) -> api.Spec:
        """Read a package spec file for the given package and version.

        Raises
            PackageNotFoundError: If the package or version does not exist
        """
        pass

    @abc.abstractmethod
    def get_package(
        self, pkg: api.Ident, options: api.OptionMap
    ) -> spfs.encoding.Digest:
        """Identify the payload for the identified binary package and build options.

        The given build options should be resolved using the package spec
        before calling this function, unless the exact complete set of options
        can be known deterministically.
        """

        pass

    @abc.abstractmethod
    def get_source_package(self, pkg: api.Ident,) -> spfs.encoding.Digest:
        """Identify the payload of a source package."""
        pass

    @abc.abstractmethod
    def publish_spec(self, spec: api.Spec) -> None:
        """Publish a package spec to this repository.

        The published spec represents all builds of a single version.
        The source package, or at least one binary package should be
        published as well in order to make the spec usable in environments.
        """
        pass

    def publish_package(
        self, pkg: api.Ident, options: api.OptionMap, digest: spfs.encoding.Digest
    ) -> None:
        """Publish a binary package to this repository.

        The published digest is expected to identify an spfs layer which contains
        the propery constructed binary package files and metadata.
        """
        pass

    def publish_source_package(
        self, pkg: api.Ident, digest: spfs.encoding.Digest
    ) -> None:
        """Publish a source package to this repository.

        The source package contains all declared source files from the package
        spec and can be used to build binary packages with any given
        set of build options.
        """

        pass
