from platform import version
from typing import Iterable, Iterator, Union
import io
import json
import posixpath
from functools import lru_cache

import spkrs
import structlog

from .. import api
from ._repository import Repository, PackageNotFoundError, VersionExistsError

_LOGGER = structlog.get_logger("spk.storage.spfs")


class SpFSRepository(Repository):
    def __init__(self, base: spkrs.SpFSRepository) -> None:
        assert isinstance(base, spkrs.SpFSRepository)
        self.rs = base

    @lru_cache()
    def list_packages(self) -> Iterable[str]:

        path = "spk/spec"
        pkgs = []
        for tag in self.rs.ls_tags(path):
            if tag.endswith("/"):
                tag = tag[:-1]
                pkgs.append(tag)
        return list(pkgs)

    @lru_cache()
    def list_package_versions(self, name: str) -> Iterable[str]:

        path = self.build_spec_tag(api.parse_ident(name))
        versions: Iterable[str] = self.rs.ls_tags(path)
        versions = map(lambda v: v.rstrip("/"), versions)
        # undo our encoding of the invalid '+' character in spfs tags
        versions = (v.replace("..", "+") for v in versions)
        return sorted(list(set(versions)))

    @lru_cache()
    def list_package_builds(self, pkg: Union[str, api.Ident]) -> Iterable[api.Ident]:

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        pkg = pkg.with_build(api.SRC)
        base = posixpath.dirname(self.build_package_tag(pkg))
        try:
            build_tags = self.rs.ls_tags(base)
        except KeyError:
            return []

        builds = []
        for build in build_tags:
            builds.append(pkg.with_build(build))
        return builds

    def force_publish_spec(self, spec: api.Spec) -> None:

        assert (
            spec.pkg.build is None or not spec.pkg.build.is_emdeded()
        ), "Cannot publish embedded package"
        meta_tag = self.build_spec_tag(spec.pkg)
        spec_data = api.write_spec(spec)
        self.rs.write_spec(meta_tag, spec_data)
        self.list_packages.cache_clear()
        self.list_package_versions.cache_clear()
        self.list_package_builds.cache_clear()

    def publish_spec(self, spec: api.Spec) -> None:

        assert spec.pkg.build is None, "Spec must be published with no build"
        meta_tag = self.build_spec_tag(spec.pkg)
        if self.rs.has_tag(meta_tag):
            # BUG(rbottriell): this creates a race condition but is not super dangerous
            # because of the non-destructive tag history
            raise VersionExistsError(spec.pkg)
        self.force_publish_spec(spec)

    @lru_cache()
    def read_spec(self, pkg: api.Ident) -> api.Spec:

        tag_str = self.build_spec_tag(pkg)
        digest = self.rs.resolve_tag_to_digest(tag_str)
        if digest is None:
            raise PackageNotFoundError(pkg) from None

        data = self.rs.read_spec(digest)
        return api.read_spec(io.StringIO(data))

    def remove_spec(self, pkg: api.Ident) -> None:

        tag_str = self.build_spec_tag(pkg)
        try:
            self.rs.remove_tag_stream(tag_str)
        except RuntimeError:
            raise PackageNotFoundError(pkg) from None
        self.list_packages.cache_clear()
        self.list_package_versions.cache_clear()
        self.list_package_builds.cache_clear()

    def publish_package(self, spec: api.Spec, digest: spkrs.Digest) -> None:

        try:
            self.read_spec(spec.pkg.with_build(None))
        except PackageNotFoundError:
            _LOGGER.debug(
                "Internal warning: version spec must be published before a specific build"
            )
        tag_string = self.build_package_tag(spec.pkg)
        self.force_publish_spec(spec)
        self.rs.push_tag(tag_string, digest)

    def get_package(self, pkg: api.Ident) -> spkrs.Digest:

        tag_str = self.build_package_tag(pkg)
        digest = self.rs.resolve_tag_to_digest(tag_str)
        if digest is None:
            raise PackageNotFoundError(tag_str) from None

        return digest

    def remove_package(self, pkg: api.Ident) -> None:

        tag_str = self.build_package_tag(pkg)
        try:
            self.rs.remove_tag_stream(tag_str)
        except RuntimeError:
            raise PackageNotFoundError(pkg) from None
        self.list_packages.cache_clear()
        self.list_package_versions.cache_clear()
        self.list_package_builds.cache_clear()

    def build_package_tag(self, pkg: api.Ident) -> str:
        """Construct an spfs tag string to represent a binary package layer."""

        assert pkg.build is not None, "Package must have associated build digest"

        tag = f"spk/pkg/{pkg}"

        # the "+" character is not a valid spfs tag character,
        # so we 'encode' it with two dots, which is not a valid sequence
        # for spk package names
        return tag.replace("+", "..")

    def build_spec_tag(self, pkg: api.Ident) -> str:
        """construct an spfs tag string to represent a spec file blob."""

        tag = f"spk/spec/{pkg}"

        # the "+" character is not a valid spfs tag character,
        # see above ^
        return tag.replace("+", "..")


def local_repository() -> SpFSRepository:
    return SpFSRepository(spkrs.local_repository())


def remote_repository(remote: str = "origin") -> SpFSRepository:
    return SpFSRepository(spkrs.remote_repository(remote))
