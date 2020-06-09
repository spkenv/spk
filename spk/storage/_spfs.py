from typing import List
import io

import spfs

from .. import api
from ._repository import Repository, PackageNotFoundError, VersionExistsError


class SpFSRepository(Repository):
    def __init__(self, spfs_repo: spfs.storage.Repository) -> None:

        self._repo = spfs_repo

    def list_packages(self) -> List[str]:

        path = "spk/spec"
        return list(self._repo.tags.ls_tags(path))

    def list_package_versions(self, name: str) -> List[str]:

        path = self.build_spec_tag(api.parse_ident(name))
        return list(self._repo.tags.ls_tags(path))

    def force_publish_spec(self, spec: api.Spec) -> None:

        meta_tag = self.build_spec_tag(spec.pkg)
        spec_data = api.write_spec(spec)
        digest = self._repo.payloads.write_payload(io.BytesIO(spec_data))
        blob = spfs.storage.Blob(payload=digest, size=len(spec_data))
        self._repo.objects.write_object(blob)
        self._repo.tags.push_tag(meta_tag, digest)

    def publish_spec(self, spec: api.Spec) -> None:

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

    def publish_package(self, pkg: api.Ident, digest: spfs.encoding.Digest) -> None:

        tag_string = self.build_package_tag(pkg)
        # TODO: sanity check if tag already exists?
        self._repo.tags.push_tag(tag_string, digest)

    def get_package(self, pkg: api.Ident) -> spfs.encoding.Digest:

        tag_str = self.build_package_tag(pkg)
        try:
            return self._repo.tags.resolve_tag(tag_str).target
        except spfs.graph.UnknownReferenceError:
            raise PackageNotFoundError(tag_str)

    def build_package_tag(self, pkg: api.Ident) -> str:
        """Construct an spfs tag string to represent a binary package layer."""

        assert pkg.build is not None, "Package must have associated build digest"

        return f"spk/pkg/{pkg}"

    def build_spec_tag(self, pkg: api.Ident) -> str:
        """construct an spfs tag string to represent a spec file blob."""

        return f"spk/spec/{pkg.with_build(None)}"


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
