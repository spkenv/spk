# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Union

import structlog

from . import storage, api

_LOGGER = structlog.get_logger("spk")


class Publisher:
    def __init__(self) -> None:

        self._from: storage.Repository = storage.local_repository()
        self._to: storage.Repository = storage.remote_repository()
        self._skip_source_packages = False
        self._force = False

    def with_source(self, repo: Union[str, storage.Repository]) -> "Publisher":

        if not isinstance(repo, storage.Repository):
            repo = storage.remote_repository(repo)
        self._from = repo
        return self

    def with_target(self, repo: Union[str, storage.Repository]) -> "Publisher":

        if not isinstance(repo, storage.Repository):
            repo = storage.remote_repository(repo)
        self._to = repo
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

            try:
                spec = self._from.read_spec(pkg)
            except storage.PackageNotFoundError:
                pass
            else:
                _LOGGER.info("publishing spec", pkg=spec.pkg)
                if self._force:
                    self._to.force_publish_spec(spec)
                else:
                    self._to.publish_spec(spec)

            builds = self._from.list_package_builds(pkg)

        else:
            builds = [pkg]

        for build in builds:

            if build.build and build.build.is_source() and self._skip_source_packages:
                _LOGGER.info("skipping source package", pkg=build)
                continue

            _LOGGER.info("publishing package", pkg=build)
            spec = self._from.read_spec(build)
            digest = self._from.get_package(build)
            if not isinstance(self._from, storage.SpFSRepository):
                _LOGGER.warn("Source is not an spfs repo, skipping package payload")
            elif not isinstance(self._to, storage.SpFSRepository):
                _LOGGER.warn("Target is not an spfs repo, skipping package payload")
            else:
                self._from.rs.push_digest(digest, self._to.rs)
            self._to.publish_package(spec, digest)
