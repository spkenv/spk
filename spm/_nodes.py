from typing import Iterable, Optional
import os

import spfs
import structlog

from . import graph, api, storage
from ._build import run_and_commit_build, build
from ._handle import BinaryPackageHandle, SourcePackageHandle

_LOGGER = structlog.get_logger("spm")


class FetchNode(graph.Node):
    """Fetch node is responsible for fetching and packaging sources."""

    def __init__(self, spec: api.Spec) -> None:

        super(FetchNode, self).__init__()
        self._spec = spec
        self.source_package = self.add_output_port(
            "source_package", SourcePackageHandle
        )

    def __str__(self) -> str:

        return f"FetchNode( pkg={self._spec.pkg} )"

    def run(self) -> None:

        _LOGGER.info("GATHERING SOURCES", pkg=self._spec.pkg)

        # TODO: better/config/cleaner
        repo = storage.SpFSRepository(spfs.get_config().get_repository())

        source_dir = f"/spfs/var/run/spm/src/{self._spec.pkg}"
        script = [f"mkdir -p {source_dir}"]
        for source in self._spec.sources:

            target_dir = source_dir
            subdir = source.subdir()
            if subdir:
                target_dir = os.path.join(source_dir, subdir.lstrip("/"))
                script.append(f"mkdir -p {target_dir}")

            # TODO: possible dependencies on utilities like git...
            script.append(source.script(target_dir))

        layer = run_and_commit_build(self._spec.pkg, "\n".join(script))
        # TODO: does this belong here, maybe another input node?
        repo.publish_spec(self._spec)
        repo.publish_source_package(self._spec.pkg, layer.digest())
        _LOGGER.info(
            "Created source package", pkg=self._spec.pkg, layer=layer.digest().str()
        )

        self.source_package.value = SourcePackageHandle(self._spec, layer.digest())


class BuildNode(graph.Node):
    def __init__(self, spec: api.Spec, options: api.OptionMap) -> None:

        super(BuildNode, self).__init__()

        self._spec = spec
        self._options = options.copy()

        self.source_package = self.add_input_port("source_package", SourcePackageHandle)
        self.binary_package = self.add_output_port(
            "binary_package", BinaryPackageHandle
        )

    def __str__(self) -> str:

        return f"BuildNode( pkg={self._spec.pkg}/{self._options.digest()} )"

    def run(self) -> None:

        _LOGGER.info("BUILDING", pkg=self._spec.pkg)
        self.binary_package.value = build(self._spec, self._options)
