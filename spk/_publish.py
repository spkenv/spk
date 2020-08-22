from typing import Union

import spfs
import structlog

from . import storage, api

_LOGGER = structlog.get_logger("spk")


class Publisher:
    def __init__(self) -> None:

        self._from = storage.local_repository()
        self._to = storage.remote_repository()
        self._skip_source_packages = False
        self._force = False

    def with_target(self, name: str) -> "Publisher":

        self._to = storage.remote_repository(name)
        return self

    def skip_source_packages(self, skip_source_packages: bool) -> "Publisher":

        self._skip_source_packages = skip_source_packages
        return self

    def force(self, force: bool) -> "Publisher":

        self._force = force
        return self

    def publish(self, pkg: Union[str, api.Ident]) -> None:

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        if pkg.build is None:

            spec = self._from.read_spec(pkg)

            _LOGGER.info("publishing spec", pkg=spec.pkg)
            if self._force:
                self._to.force_publish_spec(spec)
            else:
                self._to.publish_spec(spec)

            builds = self._from.list_package_builds(spec.pkg)

        else:
            builds = [pkg]

        for build in builds:

            if build.build and build.build.is_source() and self._skip_source_packages:
                _LOGGER.info("skipping source package", pkg=build)
                continue

            _LOGGER.info("publishing package", pkg=build)
            spec = self._from.read_spec(build)
            digest = self._from.get_package(build)
            spfs.sync_ref(
                str(digest), self._from.as_spfs_repo(), self._to.as_spfs_repo()
            )
            self._to.publish_package(spec, digest)
