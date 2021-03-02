from typing import List, Optional
import os

import structlog
import spkrs

from .. import api, storage
from ._env import data_path
from ._binary import BuildError

_LOGGER = structlog.get_logger("spk.build")


class CollectionError(BuildError):
    """Denotes a build error that happened during the collection of source files."""

    pass


class SourcePackageBuilder:
    """Builds a source package.

    >>> (
    ...     SourcePackageBuilder
    ...     .from_spec(api.Spec.from_dict({
    ...         "pkg": "my-pkg",
    ...      }))
    ...     .build()
    ... )
    my-pkg/0.0.0/src
    """

    def __init__(self) -> None:

        self._spec: Optional[api.Spec] = None
        self._repo: Optional[storage.Repository] = None

    @staticmethod
    def from_spec(spec: api.Spec) -> "SourcePackageBuilder":

        builder = SourcePackageBuilder()
        builder._spec = spec
        return builder

    def with_target_repository(
        self, repo: storage.Repository
    ) -> "SourcePackageBuilder":
        """Set the repository that the created package should be published to."""

        self._repo = repo
        return self

    def build(self) -> api.Ident:
        """Build the requested source package."""

        assert (
            self._spec is not None
        ), "Target spec not given, did you use SourcePackagebuilder.from_spec?"

        if self._repo is not None:
            repo = self._repo
        else:
            repo = storage.local_repository()

        layer = collect_and_commit_sources(self._spec)
        spec = self._spec.clone()
        spec.pkg.set_build(api.SRC)
        repo.publish_package(spec, layer)
        return spec.pkg


def collect_and_commit_sources(spec: api.Spec) -> spkrs.Digest:
    """Collect sources for the given spec and commit them into an spfs layer."""

    pkg = spec.pkg.with_build(api.SRC)

    runtime = spkrs.active_runtime()
    runtime.set_editable(True)
    spkrs.remount_runtime(runtime)
    runtime.reset("**/*")
    runtime.reset_stack()
    runtime.set_editable(True)
    spkrs.remount_runtime(runtime)

    source_dir = data_path(pkg)
    collect_sources(spec, source_dir)

    _LOGGER.info("Validating package source files...")
    spkrs.validate_source_changeset()

    return spkrs.commit_layer(runtime).digest()


def collect_sources(spec: api.Spec, source_dir: str) -> None:
    """Collect the sources for a spec in the given directory."""
    os.makedirs(source_dir)

    for source in spec.sources:

        target_dir = source_dir
        subdir = source.subdir()
        if subdir:
            target_dir = os.path.join(source_dir, subdir.lstrip("/"))
            os.makedirs(target_dir, exist_ok=True)

        source.collect(target_dir)
