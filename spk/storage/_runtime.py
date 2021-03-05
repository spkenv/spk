from typing import List, Any, Iterable, Union
import os

import spkrs

from .. import api
from ._repository import Repository, PackageNotFoundError


class RuntimeRepository(Repository):
    def list_packages(self) -> Iterable[str]:
        """Return the set of known packages in this repo."""
        try:
            return os.listdir("/spfs/spk/pkg")
        except FileNotFoundError:
            return []

    def list_package_versions(self, name: str) -> Iterable[str]:
        """Return the set of versions available for the named package."""
        try:
            return os.listdir(f"/spfs/spk/pkg/{name}")
        except FileNotFoundError:
            return []

    def list_package_builds(self, pkg: Union[str, api.Ident]) -> Iterable[api.Ident]:
        """Return the set of builds for the given package name and version."""
        if isinstance(pkg, str):
            pkg = api.parse_ident(pkg)

        try:
            builds = os.listdir(f"/spfs/spk/pkg/{pkg.name}/{pkg.version}")
        except FileNotFoundError:
            return

        for build in builds:
            if os.path.isfile(
                f"/spfs/spk/pkg/{pkg.name}/{pkg.version}/{build}/spec.yaml"
            ):
                yield pkg.with_build(build)

    def read_spec(self, pkg: api.Ident) -> api.Spec:
        """Read a package spec file for the given package, version and optional build.

        Raises
            PackageNotFoundError: If the package, version, or build does not exist
        """

        try:
            spec_file = os.path.join("/spfs/spk/pkg", str(pkg), "spec.yaml")
            return api.read_spec_file(spec_file)
        except FileNotFoundError:
            raise PackageNotFoundError(pkg)

    def get_package(self, pkg: api.Ident) -> spkrs.Digest:
        """Identify the payload for the identified binary package and build options."""

        runtime = spkrs.active_runtime()
        repo = spkrs.local_repository()

        spec_path = os.path.join("/spk/pkg", str(pkg), "spec.yaml")

        stack = runtime.get_stack()
        layers = spkrs._resolve.resolve_stack_to_layers(stack)
        manifest = spkrs.tracking.Manifest()
        for layer in reversed(layers):
            manifest = repo.read_manifest(layer.manifest).unlock()
            try:
                manifest.get_path(spec_path)
            except FileNotFoundError:
                continue
            return layer.digest()

        raise PackageNotFoundError(pkg)

    def publish_spec(self, spec: api.Spec) -> None:
        raise NotImplementedError("Cannot publish to a runtime repository")

    def remove_spec(self, pkg: api.Ident) -> None:
        raise NotImplementedError("Cannot modify a runtime repository")

    def force_publish_spec(self, spec: api.Spec) -> None:
        raise NotImplementedError("Cannot modify a runtime repository")

    def publish_package(self, spec: api.Spec, digest: spkrs.Digest) -> None:
        raise NotImplementedError("Cannot publish to a runtime repository")

    def remove_package(self, pkg: api.Ident) -> None:
        raise NotImplementedError("Cannot modify a runtime repository")
