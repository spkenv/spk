from typing import Iterable, Union
import io
import json
import posixpath

import spfs

from .. import api
from ._repository import Repository, PackageNotFoundError, VersionExistsError


class SpFSRepository(Repository):
    def __init__(self, spfs_repo: spfs.storage.Repository) -> None:

        self._repo = spfs_repo

    def __repr__(self) -> str:

        return f"SpFSRepository({self._repo.address()})"

    def as_spfs_repo(self) -> spfs.storage.Repository:
        return self._repo

    def list_packages(self) -> Iterable[str]:

        path = "spk/spec"
        return list(self._repo.tags.ls_tags(path))

    def list_package_versions(self, name: str) -> Iterable[str]:

        path = self.build_spec_tag(api.parse_ident(name))
        return list(self._repo.tags.ls_tags(path))

    def list_package_builds(self, pkg: Union[str, api.Ident]) -> Iterable[api.Ident]:

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        pkg = pkg.with_build(api.SRC)
        base = posixpath.dirname(self.build_package_tag(pkg))
        try:
            for build in self._repo.tags.ls_tags(base):
                yield pkg.with_build(build)
        except KeyError:
            return []

    def get_package_build_options(self, pkg: Union[str, api.Ident]) -> api.OptionMap:
        """Return the set of build options for a package build

        This loads the build options directly out of spfs using the
        options.json file created at build time.
        """

        import spk

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        tag_str = self.build_package_tag(pkg)
        tag = self._repo.tags.resolve_tag(tag_str)
        layer = self._repo.read_layer(tag.target)
        manifest = self._repo.read_manifest(layer.manifest).unlock()

        filepath = spk.build.build_options_path(pkg, "/")
        entry = manifest.get_path(filepath)
        with self._repo.payloads.open_payload(entry.object) as reader:
            return api.OptionMap.from_dict(json.load(reader))

    def force_publish_spec(self, spec: api.Spec) -> None:

        meta_tag = self.build_spec_tag(spec.pkg)
        spec_data = api.write_spec(spec)
        digest = self._repo.payloads.write_payload(io.BytesIO(spec_data))
        blob = spfs.storage.Blob(payload=digest, size=len(spec_data))
        self._repo.objects.write_object(blob)
        self._repo.tags.push_tag(meta_tag, digest)

    def publish_spec(self, spec: api.Spec) -> None:

        assert spec.pkg.build is None, "Spec must be published with no build"
        meta_tag = self.build_spec_tag(spec.pkg)
        if self._repo.tags.has_tag(meta_tag):
            # BUG(rbottriell): this creates a race condition but is not super dangerous
            # because of the non-destructive tag history
            raise VersionExistsError(spec.pkg)
        self.force_publish_spec(spec)

    def read_spec(self, pkg: api.Ident) -> api.Spec:

        tag_str = self.build_spec_tag(pkg)
        try:
            tag = self._repo.tags.resolve_tag(tag_str)
        except spfs.graph.UnknownReferenceError:
            raise PackageNotFoundError(pkg) from None

        with self._repo.payloads.open_payload(tag.target) as spec_file:
            return api.read_spec(spec_file)

    def remove_spec(self, pkg: api.Ident) -> None:

        tag_str = self.build_spec_tag(pkg)
        try:
            self._repo.tags.remove_tag_stream(tag_str)
        except spfs.graph.UnknownReferenceError:
            raise PackageNotFoundError(pkg) from None

    def publish_package(self, spec: api.Spec, digest: spfs.encoding.Digest) -> None:

        tag_string = self.build_package_tag(spec.pkg)
        self.force_publish_spec(spec)
        self._repo.tags.push_tag(tag_string, digest)

    def get_package(self, pkg: api.Ident) -> spfs.encoding.Digest:

        tag_str = self.build_package_tag(pkg)
        try:
            return self._repo.tags.resolve_tag(tag_str).target
        except spfs.graph.UnknownReferenceError:
            raise PackageNotFoundError(tag_str) from None

    def remove_package(self, pkg: api.Ident) -> None:

        tag_str = self.build_package_tag(pkg)
        try:
            self._repo.tags.remove_tag_stream(tag_str)
        except spfs.graph.UnknownReferenceError:
            raise PackageNotFoundError(pkg) from None

    def build_package_tag(self, pkg: api.Ident) -> str:
        """Construct an spfs tag string to represent a binary package layer."""

        assert pkg.build is not None, "Package must have associated build digest"

        return f"spk/pkg/{pkg}"

    def build_spec_tag(self, pkg: api.Ident) -> str:
        """construct an spfs tag string to represent a spec file blob."""

        return f"spk/spec/{pkg}"


def local_repository() -> SpFSRepository:
    """Return the local packages repository used for development."""

    config = spfs.get_config()
    repo = config.get_repository()
    return SpFSRepository(repo)


def remote_repository(name: str = "origin") -> SpFSRepository:
    """Return the remote repository of the given name.

    If not name is specified, return the default spfs repository.
    """

    config = spfs.get_config()
    repo = config.get_remote(name)
    return SpFSRepository(repo)
