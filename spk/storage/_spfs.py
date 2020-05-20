from typing import List
import io

import spfs

from .. import api
from ._repository import Repository, UnknownPackageError


class SpFSRepository(Repository):
    def __init__(self, spfs_repo: spfs.storage.Repository) -> None:

        self._repo = spfs_repo

    def list_packages(self) -> List[str]:

        path = "spk/spec"
        return list(self._repo.tags.ls_tags(path))

    def list_package_versions(self, name: str) -> List[str]:

        path = self.build_spec_tag(api.parse_ident(name))
        return list(self._repo.tags.ls_tags(path))

    def publish_spec(self, spec: api.Spec) -> None:

        meta_tag = self.build_spec_tag(spec.pkg)
        spec_data = api.write_spec(spec)
        digest = self._repo.payloads.write_payload(io.BytesIO(spec_data))
        blob = spfs.storage.Blob(payload=digest, size=len(spec_data))
        self._repo.objects.write_object(blob)
        # TODO: sanity check if tag already exists?
        self._repo.tags.push_tag(meta_tag, digest)

    def read_spec(self, pkg: api.Ident) -> api.Spec:
        """Read a package spec file for the given package and version.

        Raises
            UnknownPackageError: If the exact version and release does not exist
        """

        tag_str = self.build_spec_tag(pkg)
        try:
            tag = self._repo.tags.resolve_tag(tag_str)
        except spfs.graph.UnknownReferenceError:
            raise UnknownPackageError(pkg)

        with self._repo.payloads.open_payload(tag.target) as spec_file:
            return api.read_spec(spec_file)

    def publish_package(
        self, pkg: api.Ident, options: api.OptionMap, digest: spfs.encoding.Digest
    ) -> spfs.tracking.Tag:

        tag_string = self.build_binary_tag(pkg, options)
        # TODO: sanity check if tag already exists?
        return self._repo.tags.push_tag(tag_string, digest)

    def publish_source_package(
        self, pkg: api.Ident, digest: spfs.encoding.Digest
    ) -> spfs.tracking.Tag:

        tag_string = self.build_source_tag(pkg)
        # TODO: sanity check if tag already exists?
        return self._repo.tags.push_tag(tag_string, digest)

    def resolve_package(
        self, pkg: api.Ident, options: api.OptionMap
    ) -> spfs.encoding.Digest:

        tag_str = self.build_binary_tag(pkg, options)
        try:
            return self._repo.tags.resolve_tag(tag_str).target
        except spfs.graph.UnknownReferenceError:
            raise UnknownPackageError(tag_str)

    def resolve_source_package(self, pkg: api.Ident,) -> spfs.encoding.Digest:

        tag_str = self.build_source_tag(pkg)
        try:
            return self._repo.tags.resolve_tag(tag_str).target
        except spfs.graph.UnknownReferenceError:
            raise UnknownPackageError(tag_str)

    def build_binary_tag(self, pkg: api.Ident, options: api.OptionMap) -> str:
        """Construct an spfs tag string to represent a binary package layer."""

        return f"spk/pkg/{pkg}/{options.digest()}"

    def build_source_tag(self, pkg: api.Ident) -> str:
        """Construct an spfs tag string to represnet a source package layer."""

        return f"spk/src/{pkg}"

    def build_spec_tag(self, pkg: api.Ident) -> str:
        """construct an spfs tag string to represent a spec file blob."""

        return f"spk/spec/{pkg}"
